use jxl_grid::SimpleGrid;

pub fn perform_inverse_ycbcr(fb_ycbcr: [&mut SimpleGrid<f32>; 3]) {
    let [y, cb, cr] = fb_ycbcr;
    let y = y.buf_mut();
    let cb = cb.buf_mut();
    let cr = cr.buf_mut();

    for ((r, g), b) in y.iter_mut().zip(cb).zip(cr) {
        let y = *r + 0.5;
        let cb = *g;
        let cr = *b;

        *r = y + 1.402 * cr;
        *g = y - 0.344016 * cb - 0.714136 * cr;
        *b = y + 1.772 * cb;
    }
}

pub fn ycbcr_upsample(grids: [&mut SimpleGrid<f32>; 3], jpeg_upsampling: [u32; 3]) {
    fn interpolate(left: f32, center: f32, right: f32) -> (f32, f32) {
        (0.25 * left + 0.75 * center, 0.75 * center + 0.25 * right)
    }

    let shifts_ycbcr = [1, 0, 2].map(|idx| {
        jxl_modular::ChannelShift::from_jpeg_upsampling(jpeg_upsampling, idx)
    });

    for (buf, shift) in grids.into_iter().zip(shifts_ycbcr) {
        let width = buf.width();
        let height = buf.height();
        let buf = buf.buf_mut();

        let h_upsampled = shift.hshift() == 0;
        let v_upsampled = shift.vshift() == 0;

        if !h_upsampled {
            let orig_width = width;
            let width = (width + 1) / 2;
            let height = if v_upsampled { height } else { (height + 1) / 2 };

            for y in 0..height {
                let y = if v_upsampled { y } else { y * 2 };
                let idx_base = y * orig_width;
                let mut prev_sample = buf[idx_base];
                for x in 0..width {
                    let curr_sample = buf[idx_base + x * 2];
                    let right_x = if x == width - 1 { x } else { x + 1 };

                    let (me, next) = interpolate(
                        prev_sample,
                        curr_sample,
                        buf[idx_base + right_x * 2],
                    );
                    buf[idx_base + x * 2] = me;
                    if x * 2 + 1 < orig_width {
                        buf[idx_base + x * 2 + 1] = next;
                    }

                    prev_sample = curr_sample;
                }
            }
        }

        // image is horizontally upsampled here
        if !v_upsampled {
            let orig_height = height;
            let height = (height + 1) / 2;

            let mut prev_row = buf[..width].to_vec();
            for y in 0..height {
                let idx_base = y * 2 * width;
                let bottom_base = if y == height - 1 { idx_base } else { idx_base + width * 2 };
                for x in 0..width {
                    let curr_sample = buf[idx_base + x];

                    let (me, next) = interpolate(
                        prev_row[x],
                        curr_sample,
                        buf[bottom_base + x],
                    );
                    buf[idx_base + x] = me;
                    if y * 2 + 1 < orig_height {
                        buf[idx_base + width + x] = next;
                    }

                    prev_row[x] = curr_sample;
                }
            }
        }
    }
}