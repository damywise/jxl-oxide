use std::path::PathBuf;

use clap::Parser;
use jxl_oxide::{JxlImage, CropInfo, FrameBuffer, color::RenderingIntent, PixelFormat};
use lcms2::Profile;

enum LcmsTransform {
    Grayscale(lcms2::Transform<f32, f32, lcms2::GlobalContext, lcms2::AllowCache>),
    GrayscaleAlpha(lcms2::Transform<[f32; 2], [f32; 2], lcms2::GlobalContext, lcms2::AllowCache>),
    Rgb(lcms2::Transform<[f32; 3], [f32; 3], lcms2::GlobalContext, lcms2::AllowCache>),
    Rgba(lcms2::Transform<[f32; 4], [f32; 4], lcms2::GlobalContext, lcms2::AllowCache>),
}

impl LcmsTransform {
    fn transform_in_place(&self, fb: &mut FrameBuffer) {
        use LcmsTransform::*;

        match self {
            Grayscale(t) => t.transform_in_place(fb.buf_mut()),
            GrayscaleAlpha(t) => t.transform_in_place(fb.buf_grouped_mut()),
            Rgb(t) => t.transform_in_place(fb.buf_grouped_mut()),
            Rgba(t) => t.transform_in_place(fb.buf_grouped_mut()),
        }
    }
}

#[derive(Debug, Parser)]
#[command(version, about)]
struct Args {
    /// Output file
    #[arg(short, long)]
    output: Option<PathBuf>,
    /// Output ICC file
    #[arg(long)]
    icc_output: Option<PathBuf>,
    /// Input file
    input: PathBuf,
    #[arg(long, value_parser = parse_crop_info)]
    crop: Option<CropInfo>,
    #[arg(long)]
    experimental_progressive: bool,
    #[arg(short, long)]
    verbose: bool,
}

fn parse_crop_info(s: &str) -> Result<CropInfo, std::num::ParseIntError> {
    let s = s.trim();
    let mut it = s.split_whitespace().map(|s| s.parse::<u32>());
    let Some(w) = it.next().transpose()? else {
        return Ok(CropInfo {
            width: 0,
            height: 0,
            left: 0,
            top: 0,
        });
    };
    let Some(h) = it.next().transpose()? else {
        return Ok(CropInfo {
            width: w,
            height: w,
            left: 0,
            top: 0,
        });
    };
    let Some(x) = it.next().transpose()? else {
        return Ok(CropInfo {
            width: w,
            height: h,
            left: 0,
            top: 0,
        });
    };
    let Some(y) = it.next().transpose()? else {
        return Ok(CropInfo {
            width: w,
            height: w,
            left: h,
            top: x,
        });
    };
    Ok(CropInfo {
        width: w,
        height: h,
        left: x,
        top: y,
    })
}

fn main() {
    let args = Args::parse();

    let filter = if args.verbose {
        tracing::level_filters::LevelFilter::DEBUG
    } else {
        tracing::level_filters::LevelFilter::INFO
    };
    let env_filter = tracing_subscriber::EnvFilter::builder()
        .with_default_directive(filter.into())
        .from_env_lossy();
    tracing_subscriber::fmt()
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::ACTIVE)
        .with_env_filter(env_filter)
        .init();

    let span = tracing::span!(tracing::Level::TRACE, "jxl_dec (main)");
    let _guard = span.enter();

    let mut image = JxlImage::open(&args.input).expect("Failed to open file");
    let image_size = &image.image_header().size;
    let image_meta = &image.image_header().metadata;
    tracing::info!("Image dimension: {}x{}", image_size.width, image_size.height);
    tracing::debug!(colour_encoding = format_args!("{:?}", image_meta.colour_encoding));

    if let Some(icc_path) = &args.icc_output {
        if let Some(icc) = image.embedded_icc() {
            tracing::info!("Writing ICC profile");
            std::fs::write(icc_path, icc).expect("Failed to write ICC profile");
        } else {
            tracing::warn!("Input does not have embedded ICC profile, ignoring --icc-output");
        }
    }

    let mut crop = args.crop.and_then(|crop| {
        if crop.width == 0 && crop.height == 0 {
            None
        } else if crop.width == 0 {
            Some(CropInfo {
                width: image_size.width,
                ..crop
            })
        } else if crop.height == 0 {
            Some(CropInfo {
                height: image_size.height,
                ..crop
            })
        } else {
            Some(crop)
        }
    });

    if let Some(crop) = &mut crop {
        tracing::debug!(crop = format_args!("{:?}", crop), "Cropped decoding");
        let (w, h, x, y) = image_meta.apply_orientation(
            crop.width,
            crop.height,
            crop.left,
            crop.top,
            true,
        );
        crop.width = w;
        crop.height = h;
        crop.left = x;
        crop.top = y
    }

    if args.experimental_progressive {
        if let Some(path) = &args.output {
            std::fs::create_dir_all(path).expect("cannot create directory");
        }
    }

    let (width, height) = if let Some(crop) = crop {
        (crop.width, crop.height)
    } else {
        (image_size.width, image_size.height)
    };

    let decode_start = std::time::Instant::now();

    let mut keyframes = Vec::new();
    let mut renderer = image.renderer();
    loop {
        let result = renderer.render_next_frame().expect("rendering frames failed");
        match result {
            jxl_oxide::RenderResult::Done(frame) => keyframes.push(frame),
            jxl_oxide::RenderResult::NeedMoreData => panic!("Unexpected end of file"),
            jxl_oxide::RenderResult::NoMoreFrames => break,
        }
    }

    let elapsed = decode_start.elapsed();
    let elapsed_ms = elapsed.as_secs_f64() * 1000.0;
    tracing::info!("Took {:.2} ms", elapsed_ms);

    if let Some(output) = &args.output {
        // Color encoding information
        let pixfmt = renderer.pixel_format();
        let source_icc = renderer.rendered_icc();
        let embedded_icc = image.embedded_icc();
        let metadata = &image.image_header().metadata;
        let colour_encoding = &metadata.colour_encoding;
        let cicp = colour_encoding.cicp();

        let output = std::fs::File::create(output).expect("failed to open output file");
        let (width, height, _, _) = metadata.apply_orientation(width, height, 0, 0, false);
        let mut encoder = png::Encoder::new(output, width, height);

        let color_type = match pixfmt {
            PixelFormat::Gray => png::ColorType::Grayscale,
            PixelFormat::Graya => png::ColorType::GrayscaleAlpha,
            PixelFormat::Rgb => png::ColorType::Rgb,
            PixelFormat::Rgba => png::ColorType::Rgba,
            _ => {
                tracing::error!("Cannot output CMYK PNG");
                panic!();
            },
        };
        encoder.set_color(color_type);

        let sixteen_bits = metadata.bit_depth.bits_per_sample() > 8;
        if sixteen_bits {
            encoder.set_depth(png::BitDepth::Sixteen);
        } else {
            encoder.set_depth(png::BitDepth::Eight);
        }

        if let Some(animation) = &metadata.animation {
            let num_plays = animation.num_loops;
            encoder.set_animated(keyframes.len() as u32, num_plays).unwrap();
        }

        let mut transform = None;
        let icc_cicp = if let Some(icc) = embedded_icc {
            if metadata.xyb_encoded {
                let source_profile = Profile::new_icc(&source_icc).expect("Failed to create profile from jxl-oxide ICC profile");

                let target_profile = Profile::new_icc(icc);
                match target_profile {
                    Err(err) => {
                        tracing::warn!("Embedded ICC has error: {}", err);
                        None
                    },
                    Ok(target_profile) => {
                        transform = Some(match color_type {
                            png::ColorType::Grayscale => LcmsTransform::Grayscale(lcms2::Transform::new(
                                &source_profile,
                                lcms2::PixelFormat::GRAY_FLT,
                                &target_profile,
                                lcms2::PixelFormat::GRAY_FLT,
                                lcms2::Intent::RelativeColorimetric,
                            ).expect("Failed to create transform")),
                            png::ColorType::GrayscaleAlpha => LcmsTransform::GrayscaleAlpha(lcms2::Transform::new(
                                &source_profile,
                                lcms2::PixelFormat(4390924 + 128), // GRAYA_FLT
                                &target_profile,
                                lcms2::PixelFormat(4390924 + 128), // GRAYA_FLT
                                lcms2::Intent::RelativeColorimetric,
                            ).expect("Failed to create transform")),
                            png::ColorType::Rgb => LcmsTransform::Rgb(lcms2::Transform::new(
                                &source_profile,
                                lcms2::PixelFormat::RGB_FLT,
                                &target_profile,
                                lcms2::PixelFormat::RGB_FLT,
                                lcms2::Intent::RelativeColorimetric,
                            ).expect("Failed to create transform")),
                            png::ColorType::Rgba => LcmsTransform::Rgba(lcms2::Transform::new(
                                &source_profile,
                                lcms2::PixelFormat::RGBA_FLT,
                                &target_profile,
                                lcms2::PixelFormat::RGBA_FLT,
                                lcms2::Intent::RelativeColorimetric,
                            ).expect("Failed to create transform")),
                            _ => unreachable!(),
                        });

                        Some((icc, None))
                    },
                }
            } else {
                Some((icc, None))
            }
        } else if colour_encoding.is_srgb() {
            encoder.set_srgb(match colour_encoding.rendering_intent {
                RenderingIntent::Perceptual => png::SrgbRenderingIntent::Perceptual,
                RenderingIntent::Relative => png::SrgbRenderingIntent::RelativeColorimetric,
                RenderingIntent::Saturation => png::SrgbRenderingIntent::Saturation,
                RenderingIntent::Absolute => png::SrgbRenderingIntent::AbsoluteColorimetric,
            });

            None
        } else {
            // TODO: emit gAMA and cHRM
            Some((&*source_icc, cicp))
        };
        encoder.validate_sequence(true);

        let mut writer = encoder
            .write_header()
            .expect("failed to write header");

        if let Some((icc, cicp)) = &icc_cicp {
            tracing::debug!("Embedding ICC profile");
            let compressed_icc = miniz_oxide::deflate::compress_to_vec_zlib(icc, 7);
            let mut iccp_chunk_data = vec![b'0', 0, 0];
            iccp_chunk_data.extend(compressed_icc);
            writer.write_chunk(png::chunk::iCCP, &iccp_chunk_data).expect("failed to write iCCP");

            if let Some(cicp) = *cicp {
                tracing::debug!(cicp = format_args!("{:?}", cicp), "Writing cICP chunk");
                writer.write_chunk(png::chunk::ChunkType([b'c', b'I', b'C', b'P']), &cicp).expect("failed to write cICP");
            }
        }

        tracing::debug!("Writing image data");
        for keyframe in keyframes {
            if let Some(animation) = &metadata.animation {
                let duration = keyframe.duration();
                let numer = animation.tps_denominator * duration;
                let denom = animation.tps_numerator;
                let (numer, denom) = if numer >= 0x10000 || denom >= 0x10000 {
                    if duration == 0xffffffff {
                        tracing::warn!(numer, denom, "Writing multi-page image in APNG");
                    } else {
                        tracing::warn!(numer, denom, "Frame duration is not representable in APNG");
                    }
                    let duration = (numer as f32 / denom as f32) * 65535.0;
                    (duration as u16, 0xffffu16)
                } else {
                    (numer as u16, denom as u16)
                };
                writer.set_frame_delay(numer, denom).unwrap();
            }

            let mut fb = keyframe.image();
            if let Some(transform) = &transform {
                transform.transform_in_place(&mut fb);
            }

            if sixteen_bits {
                let mut buf = vec![0u8; fb.width() * fb.height() * fb.channels() * 2];
                for (b, s) in buf.chunks_exact_mut(2).zip(fb.buf()) {
                    let w = (*s * 65535.0).clamp(0.0, 65535.0) as u16;
                    let [b0, b1] = w.to_be_bytes();
                    b[0] = b0;
                    b[1] = b1;
                }
                writer.write_image_data(&buf).expect("failed to write frame");
            } else {
                let mut buf = vec![0u8; fb.width() * fb.height() * fb.channels()];
                for (b, s) in buf.iter_mut().zip(fb.buf()) {
                    *b = (*s * 255.0).clamp(0.0, 255.0) as u8;
                }
                writer.write_image_data(&buf).expect("failed to write frame");
            }
        }

        writer.finish().expect("failed to finish writing png");
    } else {
        tracing::info!("No output path specified, skipping PNG encoding");
    };
}