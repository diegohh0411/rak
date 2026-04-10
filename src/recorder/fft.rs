const SAMPLE_RATE: f64 = 44100.0;

pub fn next_pow2(n: usize) -> usize {
    let mut p = 1;
    while p < n {
        p *= 2;
    }
    p
}

pub fn fft(real: &mut [f64], imag: &mut [f64]) {
    let n = real.len();
    if n <= 1 {
        return;
    }

    let mut even_r: Vec<f64> = real.iter().step_by(2).copied().collect();
    let mut even_i: Vec<f64> = imag.iter().step_by(2).copied().collect();
    let mut odd_r: Vec<f64> = real.iter().skip(1).step_by(2).copied().collect();
    let mut odd_i: Vec<f64> = imag.iter().skip(1).step_by(2).copied().collect();

    fft(&mut even_r, &mut even_i);
    fft(&mut odd_r, &mut odd_i);

    for k in 0..n / 2 {
        let angle = -2.0 * std::f64::consts::PI * k as f64 / n as f64;
        let cos_a = angle.cos();
        let sin_a = angle.sin();
        let wr = cos_a * odd_r[k] - sin_a * odd_i[k];
        let wi = cos_a * odd_i[k] + sin_a * odd_r[k];

        real[k] = even_r[k] + wr;
        imag[k] = even_i[k] + wi;
        real[k + n / 2] = even_r[k] - wr;
        imag[k + n / 2] = even_i[k] - wi;
    }
}

pub fn magnitude_spectrum(samples: &[f64]) -> Vec<f64> {
    let n = next_pow2(samples.len());
    let mut real = vec![0.0; n];
    let mut imag = vec![0.0; n];
    real[..samples.len()].copy_from_slice(samples);

    fft(&mut real, &mut imag);

    let half = n / 2;
    let mut mags = vec![0.0; half];
    for i in 0..half {
        mags[i] = (real[i] * real[i] + imag[i] * imag[i]).sqrt();
    }
    mags
}

pub fn analyze_bands(samples: &[f64], num_bands: usize) -> Vec<f64> {
    let mags = magnitude_spectrum(samples);
    let n = next_pow2(samples.len());
    let bin_hz = SAMPLE_RATE / n as f64;

    let min_freq = 80.0;
    let max_freq = 8000.0;
    let ratio: f64 = max_freq / min_freq;

    let mut bands = vec![0.0; num_bands];

    for i in 0..num_bands {
        let lo = min_freq * ratio.powf(i as f64 / num_bands as f64);
        let hi = min_freq * ratio.powf((i + 1) as f64 / num_bands as f64);
        let lo_bin = (lo / bin_hz) as usize;
        let hi_bin = ((hi / bin_hz) as usize).min(mags.len() - 1);

        let mut sum = 0.0;
        let mut count = 0usize;
        for b in lo_bin..=hi_bin {
            if b < mags.len() {
                sum += mags[b];
                count += 1;
            }
        }
        if count > 0 {
            bands[i] = sum / count as f64;
        }
    }

    let max_band = bands.iter().cloned().fold(0.001f64, f64::max);
    for b in &mut bands {
        *b = (*b / max_band).min(1.0);
    }

    bands
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_pow2_powers_of_two() {
        assert_eq!(next_pow2(1), 1);
        assert_eq!(next_pow2(2), 2);
        assert_eq!(next_pow2(3), 4);
        assert_eq!(next_pow2(5), 8);
        assert_eq!(next_pow2(2048), 2048);
    }

    #[test]
    fn magnitude_spectrum_length() {
        let samples = vec![0.0f64; 2048];
        let mags = magnitude_spectrum(&samples);
        assert_eq!(mags.len(), 1024);
    }

    #[test]
    fn magnitude_spectrum_dc_component() {
        let samples = vec![1.0f64; 2048];
        let mags = magnitude_spectrum(&samples);
        assert!(mags[0] > 100.0);
        for i in 1..mags.len() {
            assert!(mags[i] < 1.0);
        }
    }

    #[test]
    fn analyze_bands_length_matches_n() {
        let samples = vec![0.5f64; 2048];
        let bands = analyze_bands(&samples, 32);
        assert_eq!(bands.len(), 32);
    }

    #[test]
    fn analyze_bands_all_between_zero_and_one() {
        let samples: Vec<f64> = (0..2048).map(|i| (i as f64).sin()).collect();
        let bands = analyze_bands(&samples, 16);
        for &b in &bands {
            assert!((0.0..=1.0).contains(&b));
        }
    }
}
