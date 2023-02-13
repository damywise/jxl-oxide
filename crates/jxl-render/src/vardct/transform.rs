use jxl_grid::Grid;
use jxl_vardct::TransformType;

use crate::dct::idct_2d_in_place;

fn aux_idct2_in_place(block: &mut Grid<f32>, size: u32) {
    debug_assert!(size.is_power_of_two());

    let num_2x2 = size / 2;
    let mut scratch = vec![0.0f32; (size * size) as usize];
    for y in 0..num_2x2 {
        for x in 0..num_2x2 {
            let c00 = block[(x, y)];
            let c01 = block[(x + num_2x2, y)];
            let c10 = block[(x, y + num_2x2)];
            let c11 = block[(x + num_2x2, y + num_2x2)];

            let base_idx = 2 * (y * size + x) as usize;
            scratch[base_idx] = c00 + c01 + c10 + c11;
            scratch[base_idx + 1] = c00 + c01 - c10 - c11;
            scratch[base_idx + size as usize] = c00 - c01 + c10 - c11;
            scratch[base_idx + size as usize + 1] = c00 - c01 - c10 + c11;
        }
    }

    for y in 0..size {
        for x in 0..size {
            block[(x, y)] = scratch[(y * size + x) as usize];
        }
    }
}

fn transform_dct2(coeff: &mut Grid<f32>) {
    aux_idct2_in_place(coeff, 2);
    aux_idct2_in_place(coeff, 4);
    aux_idct2_in_place(coeff, 8);
}

fn transform_dct4(coeff: &mut Grid<f32>) {
    aux_idct2_in_place(coeff, 2);

    let mut scratch = [0.0f32; 64];
    for y in 0..2 {
        for x in 0..2 {
            let scratch = &mut scratch[(y * 2 + x) as usize * 16..][..16];
            for iy in 0..4 {
                for ix in 0..4 {
                    scratch[(iy * 4 + ix) as usize] = coeff[(x + ix * 2, y + iy * 2)];
                }
            }
            idct_2d_in_place(scratch, 4, 4);
        }
    }

    for y in 0..2 {
        for x in 0..2 {
            let scratch = &mut scratch[(y * 2 + x) as usize * 16..][..16];
            for iy in 0..4 {
                for ix in 0..4 {
                    coeff[(x + ix * 2, y + iy * 2)] = scratch[(iy * 4 + ix) as usize];
                }
            }
        }
    }
}

fn transform_hornuss(coeff: &mut Grid<f32>) {
    aux_idct2_in_place(coeff, 2);

    let mut scratch = [0.0f32; 64];
    for y in 0..2 {
        for x in 0..2 {
            let scratch = &mut scratch[(y * 2 + x) as usize * 16..][..16];
            for iy in 0..4 {
                for ix in 0..4 {
                    scratch[(iy * 4 + ix) as usize] = coeff[(x + ix * 2, y + iy * 2)];
                }
            }
            let residual_sum: f32 = scratch[1..].iter().copied().sum();
            let avg = scratch[0] - residual_sum / 16.0;
            scratch[0] = scratch[5] + avg;
            scratch[5] = avg;
            for (idx, s) in scratch.iter_mut().enumerate() {
                if idx == 0 || idx == 5 {
                    continue;
                }
                *s += avg;
            }
        }
    }

    for y in 0..2 {
        for x in 0..2 {
            let scratch = &mut scratch[(y * 2 + x) as usize * 16..][..16];
            for iy in 0..4 {
                for ix in 0..4 {
                    coeff[(x + ix * 2, y + iy * 2)] = scratch[(iy * 4 + ix) as usize];
                }
            }
        }
    }
}

fn transform_dct4x8(coeff: &mut Grid<f32>, transpose: bool) {
    let coeff0 = coeff[(0, 0)];
    let coeff1 = coeff[(0, 1)];
    coeff[(0, 0)] = coeff0 + coeff1;
    coeff[(0, 1)] = coeff0 - coeff1;

    let (w, h) = if transpose {
        (4, 8)
    } else {
        (8, 4)
    };

    let mut scratch = [0.0f32; 64];
    for idx in [0, 1] {
        let scratch = &mut scratch[(idx * 32)..][..32];
        for iy in 0..4 {
            for ix in 0..8 {
                scratch[iy * 8 + ix] = coeff[(ix as u32, iy as u32)];
            }
        }
        idct_2d_in_place(scratch, w, h);
    }

    if transpose {
        for idx in [0, 4] {
            for y in 0..8 {
                for x in 0..4 {
                    coeff[((x + idx) as u32, y as u32)] = scratch[y * 4 + x + idx * 8];
                }
            }
        }
    } else {
        for y in 0..8 {
            for x in 0..8 {
                coeff[(x as u32, y as u32)] = scratch[y * 8 + x];
            }
        }
    }
}

fn transform_afv<const N: usize>(coeff: &mut Grid<f32>) {
    const AFV_BASIS: [[f32; 16]; 16] = [
        [
            0.25,
            0.876902929799142,
            0.0,
            0.0,
            0.0,
            -0.4105377591765233,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
        ],
        [
            0.25,
            0.2206518106944235,
            0.0,
            0.0,
            -0.7071067811865474,
            0.6235485373547691,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
        ],
        [
            0.25,
            -0.10140050393753763,
            0.40670075830260755,
            -0.21255748058288748,
            0.0,
            -0.06435071657946274,
            -0.4517556589999482,
            -0.304684750724869,
            0.3017929516615495,
            0.40824829046386274,
            0.1747866975480809,
            -0.21105601049335784,
            -0.14266084808807264,
            -0.13813540350758585,
            -0.17437602599651067,
            0.11354987314994337
        ],
        [
            0.25,
            -0.1014005039375375,
            0.44444816619734445,
            0.3085497062849767,
            0.0,
            -0.06435071657946266,
            0.15854503551840063,
            0.5112616136591823,
            0.25792362796341184,
            0.0,
            0.0812611176717539,
            0.18567180916109802,
            -0.3416446842253372,
            0.3302282550303788,
            0.0702790691196284,
            -0.07417504595810355
        ],
        [
            0.25,
            0.2206518106944236,
            0.0,
            0.0,
            0.7071067811865476,
            0.6235485373547694,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0
        ],
        [
            0.25,
            -0.10140050393753777,
            0.0,
            0.4706702258572536,
            0.0,
            -0.06435071657946284,
            -0.04038515160822202,
            0.0,
            0.16272340142866204,
            0.0,
            0.0,
            0.0,
            0.7367497537172237,
            0.08755115000587084,
            -0.2921026642334881,
            0.19402893032594343
        ],
        [
            0.25,
            -0.10140050393753772,
            0.19574399372042936,
            -0.1621205195722993,
            0.0,
            -0.0643507165794628,
            0.0074182263792423875,
            -0.290480129728998,
            0.09520022653475037,
            0.0,
            -0.3675398009862027,
            0.49215859013738733,
            0.24627107722075148,
            -0.07946706605909573,
            0.3623817333531167,
            -0.435190496523228
        ],
        [
            0.25,
            -0.10140050393753763,
            0.2929100136981264,
            0.0,
            0.0,
            -0.06435071657946274,
            0.39351034269210167,
            -0.06578701549142804,
            0.0,
            -0.4082482904638628,
            -0.307882213957909,
            -0.38525013709251915,
            -0.08574019035519306,
            -0.4613374887461511,
            0.0,
            0.21918684838857466
        ],
        [
            0.25,
            -0.10140050393753758,
            -0.40670075830260716,
            -0.21255748058287047,
            0.0,
            -0.06435071657946272,
            -0.45175565899994635,
            0.304684750724884,
            0.3017929516615503,
            -0.4082482904638635,
            -0.17478669754808135,
            0.21105601049335806,
            -0.14266084808807344,
            -0.13813540350758294,
            -0.1743760259965108,
            0.11354987314994257
        ],
        [
            0.25,
            -0.10140050393753769,
            -0.19574399372042872,
            -0.16212051957228327,
            0.0,
            -0.06435071657946279,
            0.007418226379244351,
            0.2904801297290076,
            0.09520022653475055,
            0.0,
            0.3675398009862011,
            -0.49215859013738905,
            0.24627107722075137,
            -0.07946706605910261,
            0.36238173335311646,
            -0.4351904965232251
        ],
        [
            0.25,
            -0.1014005039375375,
            0.0,
            -0.47067022585725277,
            0.0,
            -0.06435071657946266,
            0.1107416575309343,
            0.0,
            -0.16272340142866173,
            0.0,
            0.0,
            0.0,
            0.14883399227113567,
            0.49724647109535086,
            0.29210266423348785,
            0.5550443808910661
        ],
        [
            0.25,
            -0.10140050393753768,
            0.11379074460448091,
            -0.1464291867126764,
            0.0,
            -0.06435071657946277,
            0.08298163094882051,
            -0.23889773523344604,
            -0.35312385449816297,
            -0.40824829046386296,
            0.4826689115059883,
            0.17419412659916217,
            -0.04768680350229251,
            0.12538059448563663,
            -0.4326608024727445,
            -0.25468277124066463
        ],
        [
            0.25,
            -0.10140050393753768,
            -0.44444816619734384,
            0.3085497062849487,
            0.0,
            -0.06435071657946277,
            0.15854503551839705,
            -0.5112616136592012,
            0.25792362796341295,
            0.0,
            -0.08126111767175039,
            -0.18567180916109904,
            -0.3416446842253373,
            0.3302282550303805,
            0.07027906911962818,
            -0.07417504595810233
        ],
        [
            0.25,
            -0.10140050393753759,
            -0.29291001369812636,
            0.0,
            0.0,
            -0.06435071657946273,
            0.3935103426921022,
            0.06578701549142545,
            0.0,
            0.4082482904638634,
            0.30788221395790305,
            0.3852501370925211,
            -0.08574019035519267,
            -0.4613374887461554,
            0.0,
            0.2191868483885728
        ],
        [
            0.25,
            -0.10140050393753763,
            -0.1137907446044814,
            -0.14642918671266536,
            0.0,
            -0.06435071657946274,
            0.0829816309488214,
            0.23889773523345467,
            -0.3531238544981624,
            0.408248290463863,
            -0.48266891150598584,
            -0.1741941265991621,
            -0.047686803502292804,
            0.12538059448564315,
            -0.4326608024727457,
            -0.25468277124066413
        ],
        [
            0.25,
            -0.10140050393753741,
            0.0,
            0.4251149611657548,
            0.0,
            -0.0643507165794626,
            -0.45175565899994796,
            0.0,
            -0.6035859033230976,
            0.0,
            0.0,
            0.0,
            -0.14266084808807242,
            -0.13813540350758452,
            0.34875205199302267,
            0.1135498731499429
        ]
    ];

    assert!(N < 4);
    let flip_x = N % 2;
    let flip_y = N / 2;

    let mut coeff_afv = [0.0f32; 16];
    coeff_afv[0] = (coeff[(0, 0)] + coeff[(0, 1)] + coeff[(1, 0)]) * 4.0;
    for (idx, v) in coeff_afv.iter_mut().enumerate().skip(1) {
        let iy = idx / 4;
        let ix = idx % 4;
        *v = coeff[(2 * ix as u32, 2 * iy as u32)];
    }

    let mut samples_afv = [0.0f32; 16];
    for (idx, sample) in samples_afv.iter_mut().enumerate() {
        *sample = coeff_afv
            .into_iter()
            .zip(AFV_BASIS[idx])
            .map(|(coeff, basis)| coeff * basis)
            .sum();
    }

    let mut scratch_4x4 = coeff_afv;
    let mut scratch_4x8 = [0.0f32; 32];

    scratch_4x4[0] = coeff[(0, 0)] - coeff[(0, 1)] + coeff[(1, 0)];
    for iy in 0..4 {
        for ix in 0..4 {
            if ix + iy == 0 {
                continue;
            }
            scratch_4x4[iy * 4 + ix] = coeff[(2 * ix as u32 + 1, 2 * iy as u32)];
        }
    }
    idct_2d_in_place(&mut scratch_4x4, 4, 4);

    scratch_4x8[0] = coeff[(0, 0)] - coeff[(0, 1)] + coeff[(1, 0)];
    for iy in 0..4 {
        for ix in 0..8 {
            if ix + iy == 0 {
                continue;
            }
            scratch_4x8[iy * 8 + ix] = coeff[(ix as u32, 2 * iy as u32 + 1)];
        }
    }
    idct_2d_in_place(&mut scratch_4x8, 8, 4);

    for iy in 0..4 {
        let afv_y = if flip_y == 0 { iy } else { 3 - iy };
        for ix in 0..4 {
            let afv_x = if flip_x == 0 { ix } else { 3 - ix };
            coeff[((flip_x * 4 + ix) as u32, (flip_y * 4 + iy) as u32)] = samples_afv[afv_y * 4 + afv_x];
        }
    }

    for iy in 0..4 {
        let y = flip_y * 4 + iy;
        for ix in 0..4 {
            let x = (1 - flip_x) * 4 + ix;
            coeff[(x as u32, y as u32)] = scratch_4x4[iy * 4 + ix];
        }
    }

    for iy in 0..4 {
        let y = (1 - flip_y) * 4 + iy;
        for ix in 0..8 {
            coeff[(ix as u32, y as u32)] = scratch_4x8[iy * 4 + ix];
        }
    }
}

fn transform_dct(coeff: &mut Grid<f32>, target_width: u32, target_height: u32) {
    let width = coeff.width();
    let height = coeff.height();
    let mut scratch = vec![0.0f32; (width * height) as usize];
    for y in 0..height {
        for x in 0..width {
            scratch[(y * width + x) as usize] = coeff[(x, y)];
        }
    }
    idct_2d_in_place(&mut scratch, target_width as usize, target_height as usize);

    if width != target_width {
        *coeff = Grid::new(target_width, target_height, (target_width, target_height));
    }
    for y in 0..target_height {
        for x in 0..target_width {
            coeff[(x, y)] = scratch[(y * target_width + x) as usize];
        }
    }
}

pub fn transform(coeff: &mut Grid<f32>, dct_select: TransformType) {
    use TransformType::*;

    match dct_select {
        Dct2 => transform_dct2(coeff),
        Dct4 => transform_dct4(coeff),
        Hornuss => transform_hornuss(coeff),
        Dct4x8 => transform_dct4x8(coeff, false),
        Dct8x4 => transform_dct4x8(coeff, true),
        Afv0 => transform_afv::<0>(coeff),
        Afv1 => transform_afv::<1>(coeff),
        Afv2 => transform_afv::<2>(coeff),
        Afv3 => transform_afv::<3>(coeff),
        _ => {
            let (w8, h8) = dct_select.dct_select_size();
            transform_dct(coeff, w8 * 8, h8 * 8)
        },
    }
}