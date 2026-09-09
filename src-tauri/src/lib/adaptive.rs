use crate::compute;
use crate::compute::PixelScanResult;
use crate::params::AsciiParams;
use image::imageops::FilterType;
use image::DynamicImage;
use rayon::prelude::*;

#[derive(Clone, Copy)]
struct RegionStats {
    p_low: u8,
    p_high: u8,
}

struct GlobalLumaStats {
    p_low: u8,
    p_high: u8,
}

fn percentile_from_histogram(histogram: &[u32; 256], percentile: f32) -> u8 {
    let total: u64 = histogram.iter().map(|&count| count as u64).sum();
    if total == 0 {
        return 0;
    }
    let target = ((total as f64) * (percentile as f64)).round().max(1.0) as u64;
    let mut cumulative: u64 = 0;
    for (level, &count) in histogram.iter().enumerate() {
        cumulative += count as u64;
        if cumulative >= target {
            return level as u8;
        }
    }
    255
}

/// Глобальная статистика получается суммированием готовых региональных
/// гистограмм — отдельного прохода по пикселям для неё не нужно, регионы
/// и так разбивают все пиксели без пропусков и наложений.
fn build_global_stats(region_histograms: &[[u32; 256]], trim: f32) -> GlobalLumaStats {
    let mut histogram = [0u32; 256];
    for region in region_histograms {
        for (level, &count) in region.iter().enumerate() {
            histogram[level] += count;
        }
    }
    let p_low = percentile_from_histogram(&histogram, trim);
    let p_high = percentile_from_histogram(&histogram, 1.0 - trim).max(p_low + 1);
    GlobalLumaStats { p_low, p_high }
}

fn build_region_stats(region_histograms: &[[u32; 256]], trim: f32) -> Vec<RegionStats> {
    region_histograms
        .iter()
        .map(|histogram| {
            let p_low = percentile_from_histogram(histogram, trim);
            let p_high = percentile_from_histogram(histogram, 1.0 - trim).max(p_low + 1);
            RegionStats { p_low, p_high }
        })
        .collect()
}

fn smooth_region_ranges(
    regions: &[RegionStats],
    tiles_x: u32,
    tiles_y: u32,
    global: &GlobalLumaStats,
    min_range: f32,
    smoothing: f32,
) -> Vec<(f32, f32)> {
    let tx = tiles_x as i32;
    let ty = tiles_y as i32;
    let smoothing = smoothing.clamp(0.0, 1.0);

    (0..regions.len())
        .map(|index| {
            let col = (index as i32) % tx;
            let row = (index as i32) / tx;

            let mut lows = Vec::with_capacity(9);
            let mut highs = Vec::with_capacity(9);
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let nx = col + dx;
                    let ny = row + dy;
                    if nx < 0 || ny < 0 || nx >= tx || ny >= ty {
                        continue;
                    }
                    let neighbor = &regions[(ny * tx + nx) as usize];
                    lows.push(neighbor.p_low);
                    highs.push(neighbor.p_high);
                }
            }
            lows.sort_unstable();
            highs.sort_unstable();
            let median_low = lows[lows.len() / 2] as f32;
            let median_high = highs[highs.len() / 2] as f32;

            let own = &regions[index];
            let mut low = own.p_low as f32 * (1.0 - smoothing) + median_low * smoothing;
            let mut high = own.p_high as f32 * (1.0 - smoothing) + median_high * smoothing;

            low = low.max(global.p_low as f32 - 10.0);
            high = high.min(global.p_high as f32 + 10.0);

            if high - low < min_range {
                let center = (high + low) / 2.0;
                low = (center - min_range / 2.0).max(0.0);
                high = (center + min_range / 2.0).min(255.0);
            }
            (low, high)
        })
        .collect()
}

fn bilinear_sample_range(
    regions_ranges: &[(f32, f32)],
    tiles_x: u32,
    tiles_y: u32,
    col: u32,
    row: u32,
    columns: u32,
    rows: u32,
) -> (f32, f32) {
    let fx = ((col as f32 + 0.5) / columns.max(1) as f32) * tiles_x as f32 - 0.5;
    let fy = ((row as f32 + 0.5) / rows.max(1) as f32) * tiles_y as f32 - 0.5;

    let x0f = fx.floor();
    let y0f = fy.floor();
    let tx = (fx - x0f).clamp(0.0, 1.0);
    let ty = (fy - y0f).clamp(0.0, 1.0);

    let last_x = tiles_x as i32 - 1;
    let last_y = tiles_y as i32 - 1;
    let x0 = (x0f as i32).clamp(0, last_x.max(0)) as u32;
    let x1 = (x0f as i32 + 1).clamp(0, last_x.max(0)) as u32;
    let y0 = (y0f as i32).clamp(0, last_y.max(0)) as u32;
    let y1 = (y0f as i32 + 1).clamp(0, last_y.max(0)) as u32;

    let at = |x: u32, y: u32| regions_ranges[(y * tiles_x + x) as usize];
    let (low00, high00) = at(x0, y0);
    let (low10, high10) = at(x1, y0);
    let (low01, high01) = at(x0, y1);
    let (low11, high11) = at(x1, y1);

    let low_top = low00 + (low10 - low00) * tx;
    let low_bottom = low01 + (low11 - low01) * tx;
    let low = low_top + (low_bottom - low_top) * ty;

    let high_top = high00 + (high10 - high00) * tx;
    let high_bottom = high01 + (high11 - high01) * tx;
    let high = high_top + (high_bottom - high_top) * ty;

    (low, high)
}

fn suppress_isolated_outliers(values: &mut [f32], columns: u32, rows: u32, threshold: f32) {
    let columns_i = columns as i32;
    let rows_i = rows as i32;
    let original = values.to_vec();

    for index in 0..values.len() {
        let col = (index as i32) % columns_i;
        let row = (index as i32) / columns_i;
        let mut neighbors = Vec::with_capacity(8);
        for dy in -1..=1 {
            for dx in -1..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let nx = col + dx;
                let ny = row + dy;
                if nx < 0 || ny < 0 || nx >= columns_i || ny >= rows_i {
                    continue;
                }
                neighbors.push(original[(ny * columns_i + nx) as usize]);
            }
        }
        if neighbors.is_empty() {
            continue;
        }
        neighbors.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = neighbors[neighbors.len() / 2];
        if (original[index] - median).abs() > threshold {
            values[index] = original[index] * 0.35 + median * 0.65;
        }
    }
}

/// Общая формула яркости/контраста/инверсии для "исходных" и "целевых"
/// параметров. Та же формула продублирована в CUDA/WGSL/OpenCL-ядрах — при
/// изменении логики здесь обновите и их.
pub fn apply_brightness_contrast(value: f32, brightness: i32, contrast: i32, invert: bool) -> f32 {
    let mut adjusted =
        (value - 0.5) * (1.0 + contrast as f32 / 100.0) + 0.5 + brightness as f32 / 100.0;
    adjusted = adjusted.clamp(0.0, 1.0);
    if invert {
        adjusted = 1.0 - adjusted;
    }
    adjusted
}

pub fn normalize_and_adjust(mean: f32, low: f32, high: f32, params: &AsciiParams) -> f32 {
    let structural = ((mean - low) / (high - low).max(1.0)).clamp(0.0, 1.0);
    let mut value = apply_brightness_contrast(
        structural,
        params.source_brightness,
        params.source_contrast,
        params.source_invert,
    );
    if params.source_black_white && params.source_threshold > 0 {
        value = if value * 100.0 < params.source_threshold as f32 {
            0.0
        } else {
            1.0
        };
    }
    apply_brightness_contrast(value, params.brightness, params.contrast, params.invert)
}

pub fn normalize_cells_cpu(
    means: &[f32],
    lows: &[f32],
    highs: &[f32],
    params: &AsciiParams,
) -> Vec<f32> {
    means
        .iter()
        .zip(lows.iter())
        .zip(highs.iter())
        .map(|((&mean, &low), &high)| normalize_and_adjust(mean, low, high, params))
        .collect()
}

/// CPU-реализация тяжёлого прохода: гистограмма по каждому региону +
/// среднее по каждой ASCII-клетке. Единственная реализация, которая
/// гарантированно есть всегда (конец любой цепочки бэкендов в compute/mod.rs).
pub fn scan_pixels_cpu(
    luma: &image::GrayImage,
    columns: u32,
    rows: u32,
    tiles_x: u32,
    tiles_y: u32,
) -> PixelScanResult {
    PixelScanResult {
        region_histograms: compute_region_histograms(luma, tiles_x, tiles_y),
        cell_means: compute_cell_means(luma, columns, rows),
    }
}

fn compute_region_histograms(
    luma: &image::GrayImage,
    tiles_x: u32,
    tiles_y: u32,
) -> Vec<[u32; 256]> {
    let (width, height) = luma.dimensions();
    (0..(tiles_y * tiles_x))
        .into_par_iter()
        .map(|index| {
            let col = index % tiles_x;
            let row = index / tiles_x;
            let x0 = (width as u64 * col as u64 / tiles_x as u64) as u32;
            let x1 = ((width as u64 * (col + 1) as u64 / tiles_x as u64) as u32)
                .max(x0 + 1)
                .min(width);
            let y0 = (height as u64 * row as u64 / tiles_y as u64) as u32;
            let y1 = ((height as u64 * (row + 1) as u64 / tiles_y as u64) as u32)
                .max(y0 + 1)
                .min(height);

            let mut histogram = [0u32; 256];
            for y in y0..y1 {
                for x in x0..x1 {
                    histogram[luma.get_pixel(x, y)[0] as usize] += 1;
                }
            }
            histogram
        })
        .collect()
}

fn compute_cell_means(luma: &image::GrayImage, columns: u32, rows: u32) -> Vec<f32> {
    let (width, height) = luma.dimensions();
    (0..(rows * columns))
        .into_par_iter()
        .map(|index| {
            let col = index % columns;
            let row = index / columns;
            let x0 = (width as u64 * col as u64 / columns as u64) as u32;
            let x1 = ((width as u64 * (col + 1) as u64 / columns as u64) as u32)
                .max(x0 + 1)
                .min(width);
            let y0 = (height as u64 * row as u64 / rows as u64) as u32;
            let y1 = ((height as u64 * (row + 1) as u64 / rows as u64) as u32)
                .max(y0 + 1)
                .min(height);

            let mut sum: u64 = 0;
            let mut count: u64 = 0;
            for y in y0..y1 {
                for x in x0..x1 {
                    sum += luma.get_pixel(x, y)[0] as u64;
                    count += 1;
                }
            }
            if count > 0 {
                sum as f32 / count as f32
            } else {
                0.0
            }
        })
        .collect()
}

pub fn render_adaptive(
    luma: &image::GrayImage,
    columns: u32,
    rows: u32,
    params: &AsciiParams,
    level_count: usize,
) -> Vec<usize> {
    let tiles_x = params.adaptive_region_tiles.clamp(2, 32);
    let tiles_y = params.adaptive_region_tiles.clamp(2, 32);
    let region_trim = (params.adaptive_percentile / 100.0).clamp(0.005, 0.45);
    let global_trim = (region_trim / 2.0).clamp(0.005, 0.2);

    let scan = compute::scan_pixels(luma, columns, rows, tiles_x, tiles_y, params);

    let global = build_global_stats(&scan.region_histograms, global_trim);
    let regions = build_region_stats(&scan.region_histograms, region_trim);
    let region_ranges = smooth_region_ranges(
        &regions,
        tiles_x,
        tiles_y,
        &global,
        params.adaptive_min_range.max(4.0),
        params.adaptive_smoothing,
    );

    let means = scan.cell_means;
    let mut lows = Vec::with_capacity(means.len());
    let mut highs = Vec::with_capacity(means.len());
    for index in 0..means.len() as u32 {
        let col = index % columns;
        let row = index / columns;
        let (low, high) =
            bilinear_sample_range(&region_ranges, tiles_x, tiles_y, col, row, columns, rows);
        lows.push(low);
        highs.push(high);
    }

    let mut normalized = compute::normalize_cells(&means, &lows, &highs, params);
    suppress_isolated_outliers(
        &mut normalized,
        columns,
        rows,
        params.adaptive_despike_threshold.clamp(0.02, 0.6),
    );

    normalized
        .into_iter()
        .map(|value| ((value * level_count as f32) as usize).min(level_count.saturating_sub(1)))
        .collect()
}

/// Простое поточечное отображение после ресайза — для сравнения "было/стало".
pub fn render_legacy(
    source: &DynamicImage,
    columns: u32,
    rows: u32,
    params: &AsciiParams,
    level_count: usize,
) -> Vec<usize> {
    let resized = source
        .resize_exact(columns, rows, FilterType::Triangle)
        .to_luma8();
    resized
        .pixels()
        .map(|pixel| {
            let raw = pixel[0] as f32 / 255.0;
            let mut value = apply_brightness_contrast(
                raw,
                params.source_brightness,
                params.source_contrast,
                params.source_invert,
            );
            if params.source_black_white && params.source_threshold > 0 {
                value = if value * 100.0 < params.source_threshold as f32 {
                    0.0
                } else {
                    1.0
                };
            }
            value =
                apply_brightness_contrast(value, params.brightness, params.contrast, params.invert);
            ((value * level_count as f32) as usize).min(level_count.saturating_sub(1))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_ignores_single_pixel_outliers() {
        let mut histogram = [0u32; 256];
        for level in 100..110 {
            histogram[level] = 100;
        }
        histogram[255] = 1;
        let p_high = percentile_from_histogram(&histogram, 0.98);
        assert!(p_high < 200);
    }

    #[test]
    fn brightness_contrast_roundtrip_at_neutral_settings() {
        let value = apply_brightness_contrast(0.42, 0, 0, false);
        assert!((value - 0.42).abs() < 1e-5);
    }

    #[test]
    fn brightness_contrast_inverts_when_requested() {
        let value = apply_brightness_contrast(0.2, 0, 0, true);
        assert!((value - 0.8).abs() < 1e-5);
    }

    /// Регрессия на баг "однотонной картинки": две большие, но РАЗНЫЕ по
    /// яркости области не должны обе схлопнуться в один тон.
    #[test]
    fn adaptive_preserves_regional_brightness_difference() {
        let width = 64;
        let height = 64;
        let mut luma = image::GrayImage::new(width, height);
        for y in 0..height {
            for x in 0..width {
                let value = if x < width / 2 { 40u8 } else { 210u8 };
                luma.put_pixel(x, y, image::Luma([value]));
            }
        }

        let params = crate::params::test_params();
        let levels = params.characters.chars().count();
        let indices = render_adaptive(&luma, 20, 10, &params, levels);

        let left_avg: f32 = (0..10)
            .flat_map(|row| (0..10).map(move |col| (row, col)))
            .map(|(row, col)| indices[row * 20 + col] as f32)
            .sum::<f32>()
            / 100.0;
        let right_avg: f32 = (0..10)
            .flat_map(|row| (10..20).map(move |col| (row, col)))
            .map(|(row, col)| indices[row * 20 + col] as f32)
            .sum::<f32>()
            / 100.0;

        assert!(right_avg - left_avg > (levels as f32) * 0.3);
    }
}
