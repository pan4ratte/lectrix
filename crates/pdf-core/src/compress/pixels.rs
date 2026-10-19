//! Pixel work for Compress (ADR 0027): resampling, telling grey and black-and-white images
//! apart, a threshold for black and white, PNG predictors and packing bits.

use std::collections::HashSet;

/// How an image is resampled to fewer pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resample {
    /// A cubic filter (Catmull-Rom) widened to the reduction: sharp, as Acrobat's default.
    Bicubic,
    /// Each new pixel the average of the area it covers.
    Average,
}

/// 8-bit samples: `height` rows of `width * n` bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pixels {
    pub width: usize,
    pub height: usize,
    pub n: usize,
    pub data: Vec<u8>,
}

/// Fixed-point weights: 1.0 is `ONE`.
const SHIFT: u32 = 14;
const ONE: i32 = 1 << SHIFT;

/// For one output pixel along an axis: the first source pixel and the weights from there.
struct Taps {
    start: usize,
    weights: Vec<i32>,
}

fn catmull_rom(x: f64) -> f64 {
    let x = x.abs();
    if x < 1.0 {
        1.5 * x * x * x - 2.5 * x * x + 1.0
    } else if x < 2.0 {
        -0.5 * x * x * x + 2.5 * x * x - 4.0 * x + 2.0
    } else {
        0.0
    }
}

/// The taps that make `dst` pixels from `src` along one axis (`dst` <= `src`).
fn taps(src: usize, dst: usize, method: Resample) -> Vec<Taps> {
    let scale = src as f64 / dst as f64;
    (0..dst)
        .map(|i| {
            let mut raw: Vec<(usize, f64)> = Vec::new();
            match method {
                Resample::Average => {
                    let from = i as f64 * scale;
                    let to = (i + 1) as f64 * scale;
                    let first = from.floor() as usize;
                    let last = (to.ceil() as usize).min(src);
                    for j in first..last {
                        let overlap = (to.min((j + 1) as f64) - from.max(j as f64)).max(0.0);
                        if overlap > 0.0 {
                            raw.push((j, overlap));
                        }
                    }
                }
                Resample::Bicubic => {
                    let support = 2.0 * scale.max(1.0);
                    let center = (i as f64 + 0.5) * scale - 0.5;
                    let first = (center - support).floor() as i64 + 1;
                    let last = (center + support).floor() as i64;
                    for j in first..=last {
                        let w = catmull_rom((j as f64 - center) / scale.max(1.0));
                        if w != 0.0 {
                            let clamped = j.clamp(0, src as i64 - 1) as usize;
                            raw.push((clamped, w));
                        }
                    }
                }
            }
            if raw.is_empty() {
                raw.push((((i as f64 * scale) as usize).min(src - 1), 1.0));
            }
            let start = raw.iter().map(|(j, _)| *j).min().unwrap_or(0);
            let end = raw.iter().map(|(j, _)| *j).max().unwrap_or(0);
            let total: f64 = raw.iter().map(|(_, w)| w).sum();
            let mut weights = vec![0f64; end - start + 1];
            for (j, w) in raw {
                weights[j - start] += w / total;
            }
            let mut fixed: Vec<i32> = weights
                .iter()
                .map(|w| (w * ONE as f64).round() as i32)
                .collect();
            // The weights sum to exactly one, so flat areas stay exactly as they were.
            let error = ONE - fixed.iter().sum::<i32>();
            if let Some(biggest) = (0..fixed.len()).max_by_key(|&k| fixed[k]) {
                fixed[biggest] += error;
            }
            Taps {
                start,
                weights: fixed,
            }
        })
        .collect()
}

fn clamp_u8(sum: i32) -> u8 {
    ((sum + (1 << (SHIFT - 1))) >> SHIFT).clamp(0, 255) as u8
}

/// `src` resampled to `width` x `height`, neither larger than it is.
pub fn resample(src: &Pixels, width: usize, height: usize, method: Resample) -> Pixels {
    let width = width.clamp(1, src.width);
    let height = height.clamp(1, src.height);
    let n = src.n;
    // Across: every source row to `width` pixels.
    let across = taps(src.width, width, method);
    let mut wide = vec![0u8; src.height * width * n];
    for y in 0..src.height {
        let row = &src.data[y * src.width * n..(y + 1) * src.width * n];
        let out = &mut wide[y * width * n..(y + 1) * width * n];
        for (x, t) in across.iter().enumerate() {
            for c in 0..n {
                let mut sum = 0i32;
                for (k, w) in t.weights.iter().enumerate() {
                    sum += w * i32::from(row[(t.start + k) * n + c]);
                }
                out[x * n + c] = clamp_u8(sum);
            }
        }
    }
    // Down: those rows to `height`.
    let down = taps(src.height, height, method);
    let stride = width * n;
    let mut data = vec![0u8; height * stride];
    let mut sums = vec![0i32; stride];
    for (y, t) in down.iter().enumerate() {
        sums.fill(0);
        for (k, w) in t.weights.iter().enumerate() {
            let row = &wide[(t.start + k) * stride..(t.start + k + 1) * stride];
            for (s, v) in sums.iter_mut().zip(row) {
                *s += w * i32::from(*v);
            }
        }
        for (o, s) in data[y * stride..(y + 1) * stride].iter_mut().zip(&sums) {
            *o = clamp_u8(*s);
        }
    }
    Pixels {
        width,
        height,
        n,
        data,
    }
}

/// True if every RGB pixel is grey: its three values at most `tolerance` apart.
pub fn rgb_is_gray(data: &[u8], tolerance: u8) -> bool {
    data.as_chunks::<3>().0.iter().all(|p| {
        let (lo, hi) = (p[0].min(p[1]).min(p[2]), p[0].max(p[1]).max(p[2]));
        hi - lo <= tolerance
    })
}

/// Grey from RGB pixels that are grey already (`rgb_is_gray`): their mean.
pub fn gray_from_gray_rgb(data: &[u8]) -> Vec<u8> {
    data.as_chunks::<3>()
        .0
        .iter()
        .map(|p| ((u16::from(p[0]) + u16::from(p[1]) + u16::from(p[2]) + 1) / 3) as u8)
        .collect()
}

/// True if every grey value is black or white.
pub fn is_bitonal(gray: &[u8]) -> bool {
    gray.iter().all(|&v| v == 0 || v == 255)
}

/// True if the pixels have at most `limit` different colours: drawings and screenshots,
/// which JPEG blurs and a lossless encoding keeps sharp, and usually smaller.
pub fn few_colors(data: &[u8], n: usize, limit: usize) -> bool {
    let mut seen = HashSet::with_capacity(limit + 1);
    for p in data.chunks_exact(n) {
        let mut key = 0u32;
        for &v in p {
            key = (key << 8) | u32::from(v);
        }
        if seen.insert(key) && seen.len() > limit {
            return false;
        }
    }
    true
}

/// True if the pixels look drawn rather than photographed: a few colours (`n` > 1), or
/// most pixels exactly like the one before them (flat areas, as in screenshots, charts and
/// grey drawings). JPEG blurs their edges; a lossless encoding keeps them sharp, and is
/// usually smaller.
pub fn looks_drawn(data: &[u8], n: usize) -> bool {
    if n > 1 && few_colors(data, n, 256) {
        return true;
    }
    let pixels = data.len() / n.max(1);
    if pixels < 2 {
        return true;
    }
    let same = data
        .chunks_exact(n)
        .zip(data.chunks_exact(n).skip(1))
        .filter(|(a, b)| a == b)
        .count();
    same * 2 >= pixels
}

/// Otsu's threshold: the grey level that best separates dark from light. Values at or
/// below it are black.
pub fn otsu(gray: &[u8]) -> u8 {
    let mut histogram = [0u64; 256];
    for &v in gray {
        histogram[v as usize] += 1;
    }
    let total = gray.len() as f64;
    if total == 0.0 {
        return 127;
    }
    let sum_all: f64 = histogram
        .iter()
        .enumerate()
        .map(|(v, &c)| v as f64 * c as f64)
        .sum();
    let (mut below, mut sum_below) = (0f64, 0f64);
    let (mut best, mut best_level) = (-1f64, 127u8);
    for (level, &count) in histogram.iter().enumerate() {
        below += count as f64;
        if below == 0.0 {
            continue;
        }
        let above = total - below;
        if above == 0.0 {
            break;
        }
        sum_below += level as f64 * count as f64;
        let mean_below = sum_below / below;
        let mean_above = (sum_all - sum_below) / above;
        let between = below * above * (mean_below - mean_above).powi(2);
        if between > best {
            best = between;
            best_level = level as u8;
        }
    }
    best_level
}

/// Packs grey pixels into rows of `width.div_ceil(8)` bytes, first pixel in the high bit,
/// 1 for each value at or below `threshold` (black).
pub fn pack_black(gray: &[u8], width: usize, height: usize, threshold: u8) -> Vec<u8> {
    let stride = width.div_ceil(8);
    let mut bits = vec![0u8; stride * height];
    for y in 0..height {
        let row = &gray[y * width..(y + 1) * width];
        let out = &mut bits[y * stride..(y + 1) * stride];
        for (x, &v) in row.iter().enumerate() {
            if v <= threshold {
                out[x / 8] |= 0x80 >> (x % 8);
            }
        }
    }
    bits
}

/// Every bit flipped.
pub fn invert(bits: &[u8]) -> Vec<u8> {
    bits.iter().map(|b| !b).collect()
}

/// `rows` rows of `row_bytes` filtered with PNG predictors (PDF's /Predictor 15): each row
/// gets the filter that leaves the smallest sum of values (as libpng chooses), and a byte
/// naming it. `bpp` is the bytes per pixel, at least 1.
pub fn png_predict(data: &[u8], row_bytes: usize, rows: usize, bpp: usize) -> Vec<u8> {
    let bpp = bpp.max(1);
    let mut out = Vec::with_capacity((row_bytes + 1) * rows);
    let zero = vec![0u8; row_bytes];
    let mut candidate = vec![0u8; row_bytes];
    let mut best = vec![0u8; row_bytes];
    for y in 0..rows {
        let row = &data[y * row_bytes..(y + 1) * row_bytes];
        let up = if y == 0 {
            &zero[..]
        } else {
            &data[(y - 1) * row_bytes..y * row_bytes]
        };
        let mut best_filter = 0u8;
        let mut best_cost = u64::MAX;
        for filter in 0u8..5 {
            for i in 0..row_bytes {
                let a = if i >= bpp { row[i - bpp] } else { 0 };
                let b = up[i];
                let c = if i >= bpp { up[i - bpp] } else { 0 };
                let predicted = match filter {
                    0 => 0,
                    1 => a,
                    2 => b,
                    3 => ((u16::from(a) + u16::from(b)) / 2) as u8,
                    _ => paeth(a, b, c),
                };
                candidate[i] = row[i].wrapping_sub(predicted);
            }
            let cost: u64 = candidate
                .iter()
                .map(|&v| u64::from((v as i8).unsigned_abs()))
                .sum();
            if cost < best_cost {
                best_cost = cost;
                best_filter = filter;
                best.copy_from_slice(&candidate);
            }
        }
        out.push(best_filter);
        out.extend_from_slice(&best);
    }
    out
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let p = i16::from(a) + i16::from(b) - i16::from(c);
    let (pa, pb, pc) = (
        (p - i16::from(a)).abs(),
        (p - i16::from(b)).abs(),
        (p - i16::from(c)).abs(),
    );
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unpredict(data: &[u8], row_bytes: usize, bpp: usize) -> Vec<u8> {
        let mut out: Vec<u8> = Vec::new();
        for (y, chunk) in data.chunks_exact(row_bytes + 1).enumerate() {
            let filter = chunk[0];
            let mut row = vec![0u8; row_bytes];
            for i in 0..row_bytes {
                let a = if i >= bpp { row[i - bpp] } else { 0 };
                let b = if y > 0 {
                    out[(y - 1) * row_bytes + i]
                } else {
                    0
                };
                let c = if y > 0 && i >= bpp {
                    out[(y - 1) * row_bytes + i - bpp]
                } else {
                    0
                };
                let predicted = match filter {
                    0 => 0,
                    1 => a,
                    2 => b,
                    3 => ((u16::from(a) + u16::from(b)) / 2) as u8,
                    _ => paeth(a, b, c),
                };
                row[i] = chunk[1 + i].wrapping_add(predicted);
            }
            out.extend_from_slice(&row);
        }
        out
    }

    #[test]
    fn predicted_rows_decode_to_the_same_bytes() {
        let data: Vec<u8> = (0..30 * 7)
            .map(|i| ((i * 37 + i / 30 * 11) % 251) as u8)
            .collect();
        for bpp in [1, 3, 4] {
            let predicted = png_predict(&data, 30, 7, bpp);
            assert_eq!(predicted.len(), 31 * 7);
            assert_eq!(unpredict(&predicted, 30, bpp), data, "bpp {bpp}");
        }
    }

    #[test]
    fn a_flat_image_stays_flat_when_resampled() {
        for method in [Resample::Bicubic, Resample::Average] {
            let src = Pixels {
                width: 97,
                height: 61,
                n: 3,
                data: [10u8, 128, 250].repeat(97 * 61),
            };
            let out = resample(&src, 31, 20, method);
            assert_eq!((out.width, out.height), (31, 20));
            assert!(
                out.data
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .all(|p| *p == [10, 128, 250]),
                "{method:?}"
            );
        }
    }

    #[test]
    fn averaging_by_two_takes_the_mean_of_each_square() {
        let src = Pixels {
            width: 4,
            height: 2,
            n: 1,
            data: vec![0, 100, 200, 200, 100, 0, 0, 0],
        };
        let out = resample(&src, 2, 1, Resample::Average);
        assert_eq!(out.data, vec![50, 100]);
    }

    #[test]
    fn resampling_never_enlarges() {
        let src = Pixels {
            width: 3,
            height: 3,
            n: 1,
            data: vec![7; 9],
        };
        let out = resample(&src, 10, 10, Resample::Bicubic);
        assert_eq!((out.width, out.height), (3, 3));
    }

    #[test]
    fn grey_and_bitonal_images_are_recognised() {
        assert!(rgb_is_gray(&[5, 5, 5, 200, 200, 200], 0));
        assert!(!rgb_is_gray(&[5, 5, 6], 0));
        assert!(rgb_is_gray(&[5, 5, 6], 1));
        assert_eq!(gray_from_gray_rgb(&[5, 5, 5, 200, 201, 202]), vec![5, 201]);
        assert!(is_bitonal(&[0, 255, 255]));
        assert!(!is_bitonal(&[0, 254]));
        assert!(few_colors(&[1, 2, 3, 1, 2, 3, 4, 5, 6], 3, 2));
        assert!(!few_colors(&[1, 2, 3, 4], 1, 3));
    }

    #[test]
    fn drawings_are_told_from_photographs() {
        let flat: Vec<u8> = (0..1000).map(|i| (i / 100 * 20) as u8).collect();
        assert!(looks_drawn(&flat, 1));
        let noisy: Vec<u8> = (0..1000u32)
            .map(|i| (i.wrapping_mul(2_654_435_761) >> 24) as u8)
            .collect();
        assert!(!looks_drawn(&noisy, 1));
        assert!(
            looks_drawn(&[1, 2, 3, 4, 5, 6].repeat(100), 3),
            "two colours"
        );
    }

    #[test]
    fn otsu_separates_two_groups() {
        let mut gray = vec![30u8; 500];
        gray.extend(vec![220u8; 500]);
        let t = otsu(&gray);
        assert!((30..220).contains(&t), "{t}");
    }

    #[test]
    fn bits_are_packed_high_first_with_black_as_one() {
        let gray = [0, 255, 255, 0, 255, 255, 255, 255, 0, 255];
        let bits = pack_black(&gray, 10, 1, 127);
        assert_eq!(bits, vec![0b1001_0000, 0b1000_0000]);
        assert_eq!(invert(&bits), vec![0b0110_1111, 0b0111_1111]);
    }
}
