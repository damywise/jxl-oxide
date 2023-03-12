use super::super::{consts, reorder, small_reorder};

pub fn dct_2d(io: &mut [f32], scratch: &mut [f32], width: usize, height: usize) {
    let mut buf = vec![0.0f32; width.max(height)];

    // Performs row DCT instead of column DCT, it should be okay
    // r x c => c x r
    let row = &mut buf[..width];
    for (y, input_row) in io.chunks_exact(width).enumerate() {
        dct(input_row, row, false);
        for (tmp_row, v) in scratch.chunks_exact_mut(height).zip(&*row) {
            tmp_row[y] = *v;
        }
    }

    // c x r => if c > r then r x c else c x r
    if width <= height {
        for (input_row, output_row) in scratch.chunks_exact(height).zip(io.chunks_exact_mut(height)) {
            dct(input_row, output_row, false);
        }
    } else {
        let col = &mut buf[..height];
        for (x, input_col) in scratch.chunks_exact(height).enumerate() {
            dct(input_col, col, false);
            for (output_row, v) in io.chunks_exact_mut(width).zip(&*col) {
                output_row[x] = *v;
            }
        }
    }
}

pub fn idct_2d(coeffs_output: &mut [f32], scratch: &mut [f32], target_width: usize, target_height: usize) {
    let width = target_width.max(target_height);
    let height = target_width.min(target_height);
    let mut buf = vec![0.0f32; width];

    // Performs row DCT instead of column DCT, it should be okay
    // r x c => c x r
    let row = &mut buf[..width];
    for (y, input_row) in coeffs_output.chunks_exact(width).enumerate() {
        dct(input_row, row, true);
        for (tmp_row, v) in scratch.chunks_exact_mut(height).zip(&*row) {
            tmp_row[y] = *v;
        }
    }

    // c x r => if c > r then r x c else c x r
    if target_height >= target_width {
        for (input_row, output_row) in scratch.chunks_exact(height).zip(coeffs_output.chunks_exact_mut(height)) {
            dct(input_row, output_row, true);
        }
    } else {
        let col = &mut buf[..height];
        for (x, input_col) in scratch.chunks_exact(height).enumerate() {
            dct(input_col, col, true);
            for (output_row, v) in coeffs_output.chunks_exact_mut(width).zip(&*col) {
                output_row[x] = *v;
            }
        }
    }
}

fn dct(input: &[f32], output: &mut [f32], inverse: bool) {
    let n = input.len();
    assert!(output.len() == n);

    if n == 0 {
        return;
    }
    if n == 1 {
        output[0] = input[0];
        return;
    }
    if n == 2 {
        output[0] = input[0] + input[1];
        output[1] = input[0] - input[1];
        if !inverse {
            output[0] /= 2.0;
            output[1] /= 2.0;
        }
        return;
    }
    assert!(n.is_power_of_two());

    let mut scratch = vec![0.0f32; n];
    let cos_sin_table = consts::cos_sin(n);
    let cos_sin_table_4n = consts::cos_sin(4 * n);

    if inverse {
        output[0] = (input[0] + input[n / 2]) / 2.0 * std::f32::consts::FRAC_1_SQRT_2;
        output[1] = (input[0] - input[n / 2]) / 2.0 * std::f32::consts::FRAC_1_SQRT_2;
        for (i, o) in input[1..n / 2].iter().zip(output[2..].iter_mut().step_by(2)) {
            *o = *i;
        }
        for (i, o) in input[n / 2 + 1..].iter().rev().zip(output[3..].iter_mut().step_by(2)) {
            *o = -*i;
        }
        for (idx, slice) in output.chunks_exact_mut(2).enumerate().skip(1) {
            let [r, i] = slice else { unreachable!() };
            let cos = cos_sin_table_4n[idx];
            let sin = cos_sin_table_4n[idx + n];
            let tr = *r * cos + *i * sin;
            let ti = *i * cos - *r * sin;
            *r = tr / 4.0;
            *i = ti / 4.0;
        }
        for idx in 1..(n / 4) {
            let lr = output[idx * 2];
            let li = output[idx * 2 + 1];
            let rr = output[n - idx * 2];
            let ri = output[n - idx * 2 + 1];

            let tr = lr + rr;
            let ti = li - ri;
            let ur = lr - rr;
            let ui = li + ri;

            let cos = cos_sin_table[idx];
            let sin = cos_sin_table[idx + n / 4];
            let vr = ur * sin - ui * cos;
            let vi = ui * sin + ur * cos;

            output[idx * 2] = tr + vr;
            output[idx * 2 + 1] = ti + vi;
            output[n - idx * 2] = tr - vr;
            output[n - idx * 2 + 1] = vi - ti;
        }
        output[n / 2] *= 2.0;
        output[n / 2 + 1] *= -2.0;
        reorder(output, &mut scratch);

        let (real, imag) = scratch.split_at_mut(n / 2);
        fft_in_place(imag, real);

        let scale = 2.0 * std::f32::consts::SQRT_2;
        let it = (0..n).step_by(4).chain((0..n).rev().step_by(4)).zip(real);
        for (idx, i) in it {
            output[idx] = *i * scale;
        }
        let it = (2..n).step_by(4).chain((0..n - 2).rev().step_by(4)).zip(imag);
        for (idx, i) in it {
            output[idx] = *i * scale;
        }
    } else {
        let it = input.iter().step_by(2).chain(input.iter().rev().step_by(2)).zip(&mut scratch);
        for (i, o) in it {
            *o = *i;
        }
        reorder(&scratch, output);

        let (real, imag) = output.split_at_mut(n / 2);
        fft_in_place(real, imag);

        let l = real[0];
        let r = imag[0];
        real[0] = l + r;
        imag[0] = l - r;

        for idx in 1..(n / 4) {
            let lr = real[idx];
            let li = imag[idx];
            let rr = real[n / 2 - idx];
            let ri = imag[n / 2 - idx];

            let tr = lr + rr;
            let ti = li - ri;
            let ur = lr - rr;
            let ui = li + ri;

            let cos = cos_sin_table[idx];
            let sin = cos_sin_table[idx + n / 4];
            let vr = ur * sin + ui * cos;
            let vi = ui * sin - ur * cos;

            real[idx] = tr + vr;
            imag[idx] = ti + vi;
            real[n / 2 - idx] = tr - vr;
            imag[n / 2 - idx] = vi - ti;
        }
        real[n / 4] *= 2.0;
        imag[n / 4] *= -2.0;

        let scale = (n as f32).recip() * std::f32::consts::FRAC_1_SQRT_2;
        for (idx, (r, i)) in real.iter_mut().zip(&mut *imag).enumerate().skip(1) {
            let cos = cos_sin_table_4n[idx];
            let sin = cos_sin_table_4n[idx + n];

            let tr = *r * cos - *i * sin;
            let ti = *r * sin + *i * cos;
            *r = tr * scale;
            *i = -ti * scale;
        }
        real[0] /= n as f32;
        imag[0] /= n as f32;
        imag[1..].reverse();
    }
}

/// Assumes that inputs are reordered.
fn fft_in_place(real: &mut [f32], imag: &mut [f32]) {
    let n = real.len();
    if n < 2 {
        return;
    }

    assert!(imag.len() == n);
    if n == 2 {
        let lr = real[0];
        let li = imag[0];
        let rr = real[1];
        let ri = imag[1];
        real[0] = lr + rr;
        imag[0] = li + ri;
        real[1] = lr - rr;
        imag[1] = li - ri;
        return;
    }

    assert!(n.is_power_of_two());
    let cos_sin_table = consts::cos_sin(n);

    let mut m;
    let mut k_iter;
    m = 1;
    k_iter = n;

    for _ in 0..n.trailing_zeros() {
        m <<= 1;
        k_iter >>= 1;

        for k in 0..k_iter {
            let k = k * m;
            for j in 0..(m / 2) {
                let cos = cos_sin_table[j * k_iter];
                let sin = cos_sin_table[j * k_iter + n / 4];

                let r = real[k + m / 2 + j];
                let i = imag[k + m / 2 + j];
                // (a + ib) (cos + isin) = (a cos - b sin) + i(b cos + a sin)
                let tr = r * cos - i * sin;
                let ti = i * cos + r * sin;
                let ur = real[k + j];
                let ui = imag[k + j];

                real[k + j] = ur + tr;
                imag[k + j] = ui + ti;
                real[k + m / 2 + j] = ur - tr;
                imag[k + m / 2 + j] = ui - ti;
            }
        }
    }
}

/// Assumes that inputs are reordered.
fn small_fft_in_place<const N: usize>(real: &mut [f32], imag: &mut [f32]) {
    if N < 2 {
        return;
    }

    assert!(real.len() >= N);
    assert!(imag.len() >= N);
    if N == 2 {
        let lr = real[0];
        let li = imag[0];
        let rr = real[1];
        let ri = imag[1];
        real[0] = lr + rr;
        imag[0] = li + ri;
        real[1] = lr - rr;
        imag[1] = li - ri;
        return;
    }

    assert!(N.is_power_of_two());
    let iters = N.trailing_zeros();
    assert!(iters <= 5);

    let cos_sin_table = consts::cos_sin_small(N);
    for it in 0..iters {
        let m = 1usize << (it + 1);
        let k_iter = N >> (it + 1);

        for k in 0..k_iter {
            let k = k * m;
            for j in 0..(m / 2) {
                let cos = cos_sin_table[j * k_iter];
                let sin = cos_sin_table[j * k_iter + N / 4];

                let ur = real[k + j];
                let ui = imag[k + j];
                let r = real[k + m / 2 + j];
                let i = imag[k + m / 2 + j];
                // (a + ib) (cos + isin) = (a cos - b sin) + i(b cos + a sin)
                let tr = r * cos - i * sin;
                let ti = i * cos + r * sin;

                real[k + j] = ur + tr;
                imag[k + j] = ui + ti;
                real[k + m / 2 + j] = ur - tr;
                imag[k + m / 2 + j] = ui - ti;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn forward_dct_small() {
        let input = [1.0, 0.0, 1.0, 0.0];
        let mut output = [0.0f32; 4];
        super::dct(&input, &mut output, false);

        let s = input.len();
        for (k, output) in output.iter().enumerate() {
            let mut exp_value = 0.0f64;
            for (n, input) in input.iter().enumerate() {
                let cos = ((k * (2 * n + 1)) as f64 / s as f64 * std::f64::consts::FRAC_PI_2).cos();
                exp_value += *input as f64 * cos;
            }
            exp_value /= s as f64;
            if k != 0 {
                exp_value *= std::f64::consts::SQRT_2;
            }

            let q_expected = (exp_value * 65536.0) as i32;
            let q_actual = (*output * 65536.0) as i32;
            assert_eq!(q_expected, q_actual);
        }
    }

    #[test]
    fn forward_dct() {
        let input = [1.0, 0.0, 1.0, 2.0, -2.0, 0.0, 1.0, 0.0];
        let mut output = [0.0f32; 8];
        super::dct(&input, &mut output, false);

        let s = input.len();
        for (k, output) in output.iter().enumerate() {
            let mut exp_value = 0.0f64;
            for (n, input) in input.iter().enumerate() {
                let cos = ((k * (2 * n + 1)) as f64 / s as f64 * std::f64::consts::FRAC_PI_2).cos();
                exp_value += *input as f64 * cos;
            }
            exp_value /= s as f64;
            if k != 0 {
                exp_value *= std::f64::consts::SQRT_2;
            }

            let q_expected = (exp_value * 65536.0) as i32;
            let q_actual = (*output * 65536.0) as i32;
            assert_eq!(q_expected, q_actual);
        }
    }

    #[test]
    fn backward_dct_small() {
        let input = [3.0, 0.2, 0.0, -1.0];
        let mut output = [0.0f32; 4];
        super::dct(&input, &mut output, true);

        let s = input.len();
        for (k, output) in output.iter().enumerate() {
            let mut exp_value = input[0] as f64;
            for (n, input) in input.iter().enumerate().skip(1) {
                let cos = ((n * (2 * k + 1)) as f64 / s as f64 * std::f64::consts::FRAC_PI_2).cos();
                exp_value += *input as f64 * cos * std::f64::consts::SQRT_2;
            }

            let q_expected = (exp_value * 65536.0) as i32;
            let q_actual = (*output * 65536.0) as i32;
            assert_eq!(q_expected, q_actual);
        }
    }

    #[test]
    fn backward_dct() {
        let input = [3.0, 0.0, 0.0, -1.0, 0.0, 0.3, 0.2, 0.0];
        let mut output = [0.0f32; 8];
        super::dct(&input, &mut output, true);

        let s = input.len();
        for (k, output) in output.iter().enumerate() {
            let mut exp_value = input[0] as f64;
            for (n, input) in input.iter().enumerate().skip(1) {
                let cos = ((n * (2 * k + 1)) as f64 / s as f64 * std::f64::consts::FRAC_PI_2).cos();
                exp_value += *input as f64 * cos * std::f64::consts::SQRT_2;
            }

            let q_expected = (exp_value * 65536.0) as i32;
            let q_actual = (*output * 65536.0) as i32;
            assert_eq!(q_expected, q_actual);
        }
    }
}