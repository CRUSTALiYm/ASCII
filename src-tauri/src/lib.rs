//! Rust-ядро приложения "Конвертер ASCII" (Tauri backend).
//!
//! Файл переработан так, чтобы существовал ОДИН конвейер обработки кадра
//! (`process_dynamic_image_to_ascii`), который одинаково используется для:
//!   - статичного изображения (`convert_image_to_ascii`);
//!   - произвольного кадра, переданного как байты (`convert_frame_to_ascii`) —
//!     этим путём в будущем идут кадры камеры и захвата экрана;
//!   - потоковой обработки источника кадр-за-кадром (`run_streaming_conversion`),
//!     на основе которой строится обработка видео/камеры/экрана без загрузки
//!     всего потока в память.
//!
//! Таким образом UI (и вообще любой вызывающий код) не содержит собственной
//! логики конвертации — он только просит "преврати вот это в ASCII с такими
//! параметрами", а какой источник дал кадр (файл, камера, экран, видео) —
//! неважно.
//!
//! --------------------------------------------------------------------------
//! ЧЕСТНОЕ ПРЕДУПРЕЖДЕНИЕ ПРО ЗАВИСИМОСТИ (wgpu / nokhwa / xcap):
//! У меня в этой среде нет ни Rust-тулчейна, ни сети, поэтому я не могу
//! скомпилировать и проверить этот файл. Сигнатуры методов wgpu/nokhwa/xcap
//! чуть отличаются между минорными версиями крейтов — возможно, при сборке
//! потребуется поправить 1-3 вызова под точную версию, которую вы добавите
//! в Cargo.toml (см. блок с рекомендуемыми версиями ниже). Сама архитектура
//! и алгоритмы (адаптивная яркость, единый движок, реестр задач) от версий
//! крейтов не зависят и переписывать их не потребуется.
//!
//! Рекомендуемые добавления в Cargo.toml:
//!   wgpu = "0.20"                                   — определение доступных GPU-бэкендов
//!   pollster = "0.3"                                — синхронное ожидание async-вызовов wgpu
//!   nokhwa = { version = "0.10", features = ["input-native"] } — камера
//!   xcap = "0.0.13"                                 — захват экрана/окон
//!
//! Про "CUDA" и "DirectX 11" из ТЗ: wgpu не даёт доступа к CUDA (это
//! отдельный вычислительный стек NVIDIA, а не графический backend) и не
//! поддерживает DirectX 11 (только DirectX 12). Честная замена — реальные
//! бэкенды wgpu, которые он умеет обнаруживать и использовать на месте:
//! Vulkan, DirectX 12, Metal, OpenGL, плюс CPU как гарантированный fallback.
//! Показывать в UI то, чего реально нет за кулисами, я не стал.
//! --------------------------------------------------------------------------

use base64::Engine;
use image::imageops::FilterType;
use image::DynamicImage;
use image::ImageReader;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_updater::UpdaterExt;
#[cfg(any(feature = "camera", feature = "screen"))]
use std::io::Cursor;

// ============================================================================
// РЕЕСТР ФОНОВЫХ ЗАДАЧ
// ============================================================================

/// Общий реестр "версий" фоновых операций.
///
/// Раньше в проекте было два отдельных `AtomicU64` (для конвертации и для
/// поиска лучших настроек), и для каждого нового вида фоновой работы
/// (кадр камеры, кадр экрана, поток видео) пришлось бы заводить ещё по
/// одному. Вместо этого — один реестр "kind -> текущая версия". Начало
/// новой операции того же вида увеличивает версию и тем самым делает все
/// более ранние вызовы "устаревшими": они видят чужую версию и сами
/// прекращают работу. Это и есть отмена, без сигналов и каналов.
struct JobRegistry {
    versions: Mutex<HashMap<String, u64>>,
}

impl JobRegistry {
    fn new() -> Self {
        Self {
            versions: Mutex::new(HashMap::new()),
        }
    }

    /// Начинает новую операцию вида `kind` и возвращает её id (версию).
    /// Одновременно "отменяет" всё, что раньше выполнялось под этим `kind`.
    fn begin(&self, kind: &str) -> u64 {
        let mut map = self.versions.lock().expect("job registry poisoned");
        let next = map.get(kind).copied().unwrap_or(0) + 1;
        map.insert(kind.to_string(), next);
        next
    }

    /// Явная отмена — по сути то же самое, что и `begin`: публикуем новую
    /// версию, чтобы всё, что ссылалось на предыдущую, посчитало себя
    /// устаревшим и остановилось на ближайшей проверке.
    fn cancel(&self, kind: &str) {
        self.begin(kind);
    }

    /// Проверка "жив ли ещё этот job", вызывается перед тяжёлыми шагами и
    /// сразу после них, чтобы отменённая операция не тратила время дальше
    /// и не перезаписывала результат более новой.
    fn is_current(&self, kind: &str, id: u64) -> bool {
        let map = self.versions.lock().expect("job registry poisoned");
        map.get(kind).copied() == Some(id)
    }
}

// ============================================================================
// ОБЩИЕ ПАРАМЕТРЫ КОНВЕРТАЦИИ
// ============================================================================

/// Параметры одного преобразования кадра в ASCII. Вынесены в отдельную
/// структуру (а не дублируются в каждом Request'е), потому что ими
/// пользуются оба входа движка — конвертация файла и конвертация "сырого"
/// кадра (камера/экран/будущее видео).
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct AsciiParams {
    job_id: u64,
    columns: u32,
    characters: String,
    /// "Целевые" яркость/контраст/инверсия — финальная стилизация уже
    /// готового ASCII-изображения (применяются после анализа структуры).
    brightness: i32,
    contrast: i32,
    invert: bool,
    /// "Исходные" настройки — эмулируют коррекцию самого источника
    /// (экспозиция, ч/б, инверсия, порог) ещё до финальной стилизации.
    source_brightness: i32,
    source_contrast: i32,
    source_invert: bool,
    source_black_white: bool,
    source_threshold: i32,
    /// Новый переключатель: адаптивный многоуровневый анализ яркости (по
    /// умолчанию включён) против старого простого поточечного отображения.
    /// Оставлен как явный флаг, а не полностью убранный старый режим — так
    /// можно быстро сравнить "было/стало" и откатиться, если на каком-то
    /// материале адаптивный алгоритм даст худший результат.
    #[serde(default = "default_true")]
    adaptive: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConvertRequest {
    path: String,
    #[serde(flatten)]
    params: AsciiParams,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FrameConvertRequest {
    /// Кадр как data-URL/base64 PNG (или сырые PNG-байты в base64) — общий
    /// формат для превью с камеры и захвата экрана.
    frame_base64: String,
    #[serde(flatten)]
    params: AsciiParams,
}

#[derive(Debug, Serialize)]
struct ConvertResult {
    columns: u32,
    rows: u32,
    values: Vec<String>,
    text: String,
}

#[derive(Debug, Serialize, Clone)]
struct ConversionProgress {
    #[serde(rename = "jobId")]
    job_id: u64,
    processed: usize,
    total: usize,
}

#[derive(Debug, Serialize)]
struct BestSettings {
    resolution: u32,
}

// ============================================================================
// АДАПТИВНЫЙ АНАЛИЗ ЯРКОСТИ
// ============================================================================
//
// Идея (см. п.1 ТЗ): глобальный диапазон яркости изображения почти всегда
// растянут на 0..255, но конкретная клетка ASCII-сетки обычно "живёт" в
// куда более узком локальном диапазоне. Если всегда нормализовать по
// глобальному диапазону — плоские полутёмные/полусветлые области теряют
// весь контраст. Если нормализовать каждую клетку независимо по своему
// min/max — единичный шумный пиксель в клетке рвёт картинку.
//
// Здесь используется трёхуровневая схема:
//   1. Глобальная робастная статистика (перцентили 2%/98%) — верхняя
//      граница "разумного" диапазона для всего изображения.
//   2. Локальная статистика по клетке — считается по гистограмме исходных
//      пикселей внутри клетки (а не после ресайза), перцентили 5%/95%.
//   3. Сглаживание по соседям (медиана окна 3x3) — цельная область получит
//      согласованный диапазон у всех своих клеток, а один "шумный" сосед не
//      перетянет контраст на себя, потому что медиана устойчива к выбросам.
//
// Финальный проход (`suppress_isolated_outliers`) — edge-preserving despike
// уже по итоговым нормализованным значениям: клетка, сильно отличающаяся от
// медианы соседей, частично подтягивается к ним (шум/выброс), а клетка в
// пределах порога — нет (реальная деталь/грань остаётся резкой).

/// Статистика одной клетки ASCII-сетки, посчитанная по исходным (ещё не
/// уменьшенным) пикселям, которые в неё попадают.
#[derive(Clone, Copy)]
struct CellStats {
    mean: f32,
    p_low: u8,
    p_high: u8,
}

/// Глобальная робастная статистика по всему изображению.
struct GlobalLumaStats {
    p_low: u8,
    p_high: u8,
}

/// Значение яркости (0..255), ниже которого лежит доля `percentile`
/// (0.0..1.0) всех отсчётов гистограммы. Используется вместо grубого
/// min/max именно затем, чтобы единичные экстремальные пиксели (шум,
/// блик, продавленная тень одного пикселя) не растягивали палитру на
/// весь диапазон.
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

/// Глобальная гистограмма и её робастный диапазон (2-й / 98-й перцентиль).
fn compute_global_stats(luma: &image::GrayImage) -> GlobalLumaStats {
    let mut histogram = [0u32; 256];
    for pixel in luma.pixels() {
        histogram[pixel[0] as usize] += 1;
    }
    let p_low = percentile_from_histogram(&histogram, 0.02);
    let p_high = percentile_from_histogram(&histogram, 0.98).max(p_low + 1);
    GlobalLumaStats { p_low, p_high }
}

/// Статистика по каждой клетке будущей ASCII-сетки (`columns` x `rows`),
/// посчитанная напрямую по пикселям исходного (полноразмерного)
/// изображения — не по уже уменьшенной картинке. Так клетка "видит" все
/// пиксели, которые реально в неё попадают, а не одно усреднённое значение
/// после чужого фильтра ресайза. Считается параллельно по клеткам (rayon),
/// поэтому суммарная стоимость линейна от количества пикселей изображения,
/// как и раньше.
fn compute_cell_stats(luma: &image::GrayImage, columns: u32, rows: u32) -> Vec<CellStats> {
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

            let mut histogram = [0u32; 256];
            let mut sum: u64 = 0;
            let mut count: u64 = 0;
            for y in y0..y1 {
                for x in x0..x1 {
                    let value = luma.get_pixel(x, y)[0];
                    histogram[value as usize] += 1;
                    sum += value as u64;
                    count += 1;
                }
            }
            let mean = if count > 0 {
                sum as f32 / count as f32
            } else {
                0.0
            };
            let p_low = percentile_from_histogram(&histogram, 0.05);
            let p_high = percentile_from_histogram(&histogram, 0.95);
            CellStats {
                mean,
                p_low,
                p_high,
            }
        })
        .collect()
}

/// Сглаживает рабочий диапазон [p_low, p_high] каждой клетки по соседям
/// (окно 3x3) через медиану — она, в отличие от среднего, устойчива к
/// единичному выбросу среди соседей. Затем подмешивает 60% "мнения
/// соседей" к 40% "собственного измерения клетки": так крупная связная
/// область получает согласованный, плавно меняющийся диапазон, а
/// единичный шумный пиксель / микро-деталь не может продавить контраст
/// целой клетки в одиночку. Дополнительно диапазон не даём вырваться
/// далеко за пределы глобального (иначе маленький контрастный фрагмент
/// "съел" бы всю палитру одной клетки) и гарантируем минимальную ширину
/// диапазона, чтобы не делить на около-ноль на плоских участках.
fn smooth_cell_ranges(
    cells: &[CellStats],
    columns: u32,
    rows: u32,
    global: &GlobalLumaStats,
) -> Vec<(f32, f32)> {
    let columns_i = columns as i32;
    let rows_i = rows as i32;
    const MIN_GAP: f32 = 30.0; // дать возможность самому редактировать

    (0..cells.len())
        .map(|index| {
            let col = (index as i32) % columns_i;
            let row = (index as i32) / columns_i;

            let mut lows = Vec::with_capacity(9);
            let mut highs = Vec::with_capacity(9);
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let nx = col + dx;
                    let ny = row + dy;
                    if nx < 0 || ny < 0 || nx >= columns_i || ny >= rows_i {
                        continue;
                    }
                    let neighbor = &cells[(ny * columns_i + nx) as usize];
                    lows.push(neighbor.p_low);
                    highs.push(neighbor.p_high);
                }
            }
            lows.sort_unstable();
            highs.sort_unstable();
            let median_low = lows[lows.len() / 2] as f32;
            let median_high = highs[highs.len() / 2] as f32;

            let own = &cells[index];
            let mut low = own.p_low as f32 * 0.4 + median_low * 0.6;
            let mut high = own.p_high as f32 * 0.4 + median_high * 0.6;

            low = low.max(global.p_low as f32 - 10.0);
            high = high.min(global.p_high as f32 + 10.0);

            if high - low < MIN_GAP {
                let center = (high + low) / 2.0;
                low = (center - MIN_GAP / 2.0).max(0.0);
                high = (center + MIN_GAP / 2.0).min(255.0);
            }
            (low, high)
        })
        .collect()
}

/// Финальный проход подавления шума по уже нормализованным (0..1)
/// значениям клеток. Если клетка сильно отличается от медианы своих
/// соседей — это, скорее всего, единичный выброс (шум сенсора, артефакт
/// сжатия), и её частично подтягивают к соседям. Если отличие в пределах
/// порога — не трогаем: так настоящие грани и силуэты остаются резкими,
/// а не размываются вместе с шумом.
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

/// Применяет регулировку яркости/контраста к нормализованному 0..1
/// значению и, при необходимости, инвертирует его. Раньше эта формула
/// была продублирована дважды подряд (для "исходных" и "целевых"
/// параметров) прямо внутри цикла по пикселям — вынесена в одну функцию,
/// чтобы поведение слайдеров на обоих уровнях гарантированно совпадало.
fn apply_brightness_contrast(value: f32, brightness: i32, contrast: i32, invert: bool) -> f32 {
    let mut adjusted =
        (value - 0.5) * (1.0 + contrast as f32 / 100.0) + 0.5 + brightness as f32 / 100.0;
    adjusted = adjusted.clamp(0.0, 1.0);
    if invert {
        adjusted = 1.0 - adjusted;
    }
    adjusted
}

/// Превращает структурную яркость клетки (мат. ожидание пикселей клетки,
/// нормализованное по её сглаженному робастному диапазону) в финальное
/// 0..1 значение, применяя пользовательские "исходные" и "целевые"
/// правки в том же порядке, что и раньше — так существующие слайдеры в
/// интерфейсе продолжат работать привычным образом поверх нового,
/// более честного анализа структуры.
fn normalize_and_adjust(mean: f32, low: f32, high: f32, params: &AsciiParams) -> f32 {
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

/// Новый адаптивный путь: глобальная статистика → локальная статистика по
/// клеткам → сглаживание по соседям → нормализация с пользовательскими
/// правками → подавление единичных выбросов → индекс символа палитры.
/// Возвращает индексы символов в row-major порядке (совместимо с
/// `values_to_text`).
fn render_adaptive(
    luma: &image::GrayImage,
    columns: u32,
    rows: u32,
    params: &AsciiParams,
    level_count: usize,
) -> Vec<usize> {
    let global = compute_global_stats(luma);
    let cells = compute_cell_stats(luma, columns, rows);
    let ranges = smooth_cell_ranges(&cells, columns, rows, &global);

    let mut normalized: Vec<f32> = cells
        .iter()
        .zip(ranges.iter())
        .map(|(cell, &(low, high))| normalize_and_adjust(cell.mean, low, high, params))
        .collect();

    suppress_isolated_outliers(&mut normalized, columns, rows, 0.12);

    normalized
        .into_iter()
        .map(|value| ((value * level_count as f32) as usize).min(level_count.saturating_sub(1)))
        .collect()
}

/// Старый путь (простое поточечное отображение после ресайза), оставлен
/// как явно выбираемый режим для сравнения "было/стало" и как быстрый
/// fallback, если адаптивный анализ на каком-то материале даст худший
/// результат.
fn render_legacy(
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

// ============================================================================
// ЕДИНЫЙ ДВИЖОК: DynamicImage -> ASCII
// ============================================================================

/// Единственная точка, где кадр (откуда бы он ни пришёл — файл, камера,
/// экран, кадр видео) превращается в ASCII-сетку. И `convert_image_to_ascii`,
/// и `convert_frame_to_ascii`, и будущая потоковая обработка видео вызывают
/// именно её — благодаря этому Preview и Export физически не могут
/// разойтись: они оба вызывают один и тот же код с одними и теми же
/// параметрами (см. п.3 ТЗ "Preview ≠ Output").
fn process_dynamic_image_to_ascii(
    app: &AppHandle,
    image: DynamicImage,
    params: &AsciiParams,
) -> Result<ConvertResult, String> {
    let levels: Vec<char> = params.characters.chars().collect();
    if levels.is_empty() {
        return Err("Набор символов не может быть пустым".into());
    }
    let columns = params.columns.max(8);
    let ratio = image.height() as f32 / image.width().max(1) as f32;
    let rows = ((columns as f32 * ratio * 0.5).round() as u32).max(4);

    // Промежуточный прогресс: сам анализ структуры для превью-разрешений
    // занимает миллисекунды, поэтому вместо прогресса "по пикселю" (как
    // было раньше) отмечаем крупные этапы — это честнее отражает, где
    // реально тратится время, и не тормозит обработку постоянными emit'ами.
    let _ = app.emit(
        "conversion-progress",
        ConversionProgress {
            job_id: params.job_id,
            processed: 0,
            total: (rows * columns) as usize,
        },
    );

    let cell_levels: Vec<usize> = if params.adaptive {
        let luma = image.to_luma8();
        render_adaptive(&luma, columns, rows, params, levels.len())
    } else {
        render_legacy(&image, columns, rows, params, levels.len())
    };

    let values: Vec<String> = cell_levels
        .iter()
        .map(|&index| levels[index].to_string())
        .collect();
    let text = values_to_text(&values, columns);

    let _ = app.emit(
        "conversion-progress",
        ConversionProgress {
            job_id: params.job_id,
            processed: cell_levels.len(),
            total: cell_levels.len(),
        },
    );

    Ok(ConvertResult {
        columns,
        rows,
        values,
        text,
    })
}

fn values_to_text(values: &[String], columns: u32) -> String {
    values
        .chunks(columns.max(1) as usize)
        .map(|row| row.concat())
        .collect::<Vec<_>>()
        .join("\n")
}

// ============================================================================
// ПОТОКОВЫЙ ИСТОЧНИК КАДРОВ (основа для видео/камеры/экрана)
// ============================================================================

/// Абстракция источника кадров. У видеофайла, камеры, захвата экрана и (в
/// перспективе) OBS разное происхождение кадров, но дальше по конвейеру
/// (анализ яркости → ASCII → рендер) они идут через один и тот же код —
/// именно эта абстракция это гарантирует. Источник отдаёт кадры по одному,
/// поэтому видео никогда не грузится в память целиком (п. "Видео" и
/// "Большие разрешения" ТЗ про потоковую обработку).
trait FrameSource {
    /// Следующий кадр, либо `None`, если поток закончился.
    fn next_frame(&mut self) -> Result<Option<DynamicImage>, String>;
    /// Частота кадров источника, если она известна.
    fn frame_rate(&self) -> Option<f32> {
        None
    }
}

/// ЗАГЛУШКА, а не притворство: декодирование видеофайлов (mp4/webm/mkv) без
/// внешней программы вроде ffmpeg требует Rust-декодера видео-кодека
/// (например, контейнер через `mp4` + кадры через `openh264`, либо другой
/// стек, который вы выберете). Я не подключаю конкретный декодер здесь,
/// потому что не могу в этой среде ни собрать его, ни проверить, что он
/// реально декодирует ваши файлы — писать код "как будто" он работает
/// значило бы выдать нерабочую заглушку за готовую фичу.
///
/// Как только решим, каким крейтом декодировать кадры, `VideoFileSource`
/// реализует тот же трейт `FrameSource`, что и `CameraFrameSource` /
/// `ScreenFrameSource` ниже — и `run_streaming_conversion`, единый движок
/// и вся отмена/прогресс заработают для видеофайлов без единой правки.
struct VideoFileSource;

impl FrameSource for VideoFileSource {
    fn next_frame(&mut self) -> Result<Option<DynamicImage>, String> {
        Err("Декодирование видеофайлов ещё не подключено — см. комментарий у VideoFileSource в lib.rs".into())
    }
}

/// Источник — кадр с камеры. Виртуальная камера OBS видна операционной
/// системе как обычное устройство захвата, поэтому отдельная интеграция с
/// OBS API не нужна: достаточно выбрать её в списке `list_camera_devices`
/// (в нём такие устройства помечены `is_probably_obs_virtual_camera`).
#[cfg(feature = "camera")]
struct CameraFrameSource {
    camera: nokhwa::Camera,
}

#[cfg(feature = "camera")]
impl FrameSource for CameraFrameSource {
    fn next_frame(&mut self) -> Result<Option<DynamicImage>, String> {
        let frame = self.camera.frame().map_err(|error| error.to_string())?;
        let decoded = frame
            .decode_image::<nokhwa::pixel_format::RgbAFormat>()
            .map_err(|error| error.to_string())?;
        Ok(Some(DynamicImage::ImageRgba8(decoded)))
    }

    fn frame_rate(&self) -> Option<f32> {
        Some(self.camera.frame_rate() as f32)
    }
}

/// Источник — кадр целого монитора (скринкаст).
#[cfg(feature = "screen")]
struct ScreenFrameSource {
    monitor: xcap::Monitor,
}

#[cfg(feature = "screen")]
impl FrameSource for ScreenFrameSource {
    fn next_frame(&mut self) -> Result<Option<DynamicImage>, String> {
        let image = self
            .monitor
            .capture_image()
            .map_err(|error| error.to_string())?;
        Ok(Some(DynamicImage::ImageRgba8(image)))
    }
}

/// Единая потоковая обработка: прогоняет источник кадр за кадром через тот
/// же движок, что и статичные изображения, эмитит прогресс и уважает
/// отмену через `JobRegistry`. На этой функции строится и live-превью
/// камеры/экрана, и (после подключения декодера) полноценная обработка
/// видеофайлов — второго конвейера в проекте не появится.
#[allow(dead_code)]
fn run_streaming_conversion(
    app: &AppHandle,
    jobs: &JobRegistry,
    job_kind: &str,
    mut source: impl FrameSource,
    params: AsciiParams,
    max_frames: Option<u32>,
    mut on_frame: impl FnMut(u32, ConvertResult),
) -> Result<(), String> {
    let job_id = jobs.begin(job_kind);
    let mut frame_index: u32 = 0;

    loop {
        if !jobs.is_current(job_kind, job_id) {
            return Err("stream_cancelled".into());
        }
        if let Some(limit) = max_frames {
            if frame_index >= limit {
                break;
            }
        }
        let frame = match source.next_frame()? {
            Some(frame) => frame,
            None => break,
        };

        let mut frame_params = params.clone();
        frame_params.job_id = job_id;
        let result = process_dynamic_image_to_ascii(app, frame, &frame_params)?;
        on_frame(frame_index, result);
        frame_index += 1;

        let _ = app.emit("stream-frame-done", frame_index);
    }

    Ok(())
}

// ============================================================================
// TAURI-КОМАНДЫ: ИЗОБРАЖЕНИЯ
// ============================================================================

/// Подбирает стартовую ширину ASCII-сетки по метаданным файла, не
/// декодируя изображение целиком — декодирование крупного фото только
/// ради узнавания размеров делало "автоподбор" ощутимо подвисающим.
fn best_resolution(width: u32) -> u32 {
    (width / 8).clamp(64, 180)
}

#[tauri::command]
fn find_best_settings(
    path: String,
    job_id: u64,
    jobs: State<JobRegistry>,
) -> Result<BestSettings, String> {
    if !jobs.is_current("settings_search", job_id) {
        return Err("settings_search_cancelled".into());
    }
    let reader = ImageReader::open(path)
        .map_err(|error| error.to_string())?
        .with_guessed_format()
        .map_err(|error| error.to_string())?;
    let (width, _height) = reader
        .into_dimensions()
        .map_err(|error| error.to_string())?;
    if !jobs.is_current("settings_search", job_id) {
        return Err("settings_search_cancelled".into());
    }
    Ok(BestSettings {
        resolution: best_resolution(width),
    })
}

#[tauri::command]
fn begin_settings_search(jobs: State<JobRegistry>) -> u64 {
    jobs.begin("settings_search")
}

#[tauri::command]
fn cancel_settings_search(jobs: State<JobRegistry>) {
    jobs.cancel("settings_search");
}

#[tauri::command]
fn begin_conversion(jobs: State<JobRegistry>) -> u64 {
    jobs.begin("conversion")
}

#[tauri::command]
fn cancel_conversion(jobs: State<JobRegistry>) {
    jobs.cancel("conversion");
}

/// Общие команды для будущих потоковых источников (камера/экран/видео) —
/// принимают `kind` вместо того, чтобы плодить `begin_camera`/`cancel_camera`,
/// `begin_screen`/`cancel_screen` и так далее по одному на источник.
#[tauri::command]
fn begin_job(kind: String, jobs: State<JobRegistry>) -> u64 {
    jobs.begin(&kind)
}

#[tauri::command]
fn cancel_job(kind: String, jobs: State<JobRegistry>) {
    jobs.cancel(&kind);
}

#[tauri::command]
fn convert_image_to_ascii(
    app: AppHandle,
    jobs: State<JobRegistry>,
    request: ConvertRequest,
) -> Result<ConvertResult, String> {
    if !jobs.is_current("conversion", request.params.job_id) {
        return Err("conversion_cancelled".into());
    }
    let image = ImageReader::open(&request.path)
        .map_err(|error| error.to_string())?
        .decode()
        .map_err(|error| error.to_string())?;

    let result = process_dynamic_image_to_ascii(&app, image, &request.params)?;

    if !jobs.is_current("conversion", request.params.job_id) {
        return Err("conversion_cancelled".into());
    }
    Ok(result)
}

/// Тот же движок, что и `convert_image_to_ascii`, но на входе — уже
/// захваченный кадр (base64 PNG), а не путь к файлу. Используется для
/// живого превью камеры и захвата экрана: сначала кадр захватывается
/// (`capture_camera_frame` / `capture_screen_frame`), затем прогоняется
/// через эту же команду — так гарантированно нет отдельной "второй"
/// логики конвертации для не-файловых источников.
#[tauri::command]
fn convert_frame_to_ascii(
    app: AppHandle,
    jobs: State<JobRegistry>,
    request: FrameConvertRequest,
) -> Result<ConvertResult, String> {
    if !jobs.is_current("conversion", request.params.job_id) {
        return Err("conversion_cancelled".into());
    }
    let encoded = request
        .frame_base64
        .split_once(',')
        .map(|(_, data)| data)
        .unwrap_or(request.frame_base64.as_str());
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|error| error.to_string())?;
    let image = image::load_from_memory(&bytes).map_err(|error| error.to_string())?;

    let result = process_dynamic_image_to_ascii(&app, image, &request.params)?;

    if !jobs.is_current("conversion", request.params.job_id) {
        return Err("conversion_cancelled".into());
    }
    Ok(result)
}

#[tauri::command]
fn read_image_as_base64(path: String) -> Result<String, String> {
    let bytes = std::fs::read(&path).map_err(|error| error.to_string())?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
}

// ============================================================================
// TAURI-КОМАНДЫ: ДОСТУПНЫЕ RENDER-БЭКЕНДЫ (GPU/CPU)
// ============================================================================

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct RenderBackendInfo {
    /// Стабильный идентификатор для сохранения выбора пользователя
    /// (`"cpu"`, `"vulkan:0"`, `"dx12:0"`, ...).
    id: String,
    /// Человекочитаемое имя для UI, напр. "NVIDIA GeForce RTX 4070 (Vulkan)".
    label: String,
    kind: String, // "cpu" | "gpu"
    is_default: bool,
}

/// Определяет РЕАЛЬНО доступные на этой системе бэкенды рендера/вычислений
/// через перечисление адаптеров wgpu — никаких пунктов "на всякий случай":
/// если совместимого Vulkan/DX12/Metal-адаптера нет, его не будет в списке.
/// CPU присутствует всегда как гарантированный fallback (текущий движок и
/// так работает на CPU через `rayon` — этот пункт им и является).
#[cfg(feature = "gpu-backends")]
#[tauri::command]
fn list_render_backends() -> Vec<RenderBackendInfo> {
    let mut backends = vec![RenderBackendInfo {
        id: "cpu".into(),
        label: "CPU (универсальный, всегда доступен)".into(),
        kind: "cpu".into(),
        is_default: false,
    }];

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());

    for adapter in pollster::block_on(instance.enumerate_adapters(wgpu::Backends::all())) {
        let info = adapter.get_info();
        // Программные (CPU-эмулированные) адаптеры уже покрыты пунктом "cpu" —
        // не дублируем их отдельной строкой в списке.
        if info.device_type == wgpu::DeviceType::Cpu {
            continue;
        }
        let backend_id = match info.backend {
            wgpu::Backend::Vulkan => "vulkan",
            wgpu::Backend::Dx12 => "dx12",
            wgpu::Backend::Metal => "metal",
            wgpu::Backend::Gl => "gl",
            _ => continue,
        };
        backends.push(RenderBackendInfo {
            id: format!("{backend_id}:{}", info.device),
            label: format!("{} ({})", info.name, backend_label(info.backend)),
            kind: "gpu".into(),
            is_default: false,
        });
    }

    // "Auto" в UI — это отдельный пункт списка на стороне фронтенда; здесь
    // мы просто помечаем реально лучший вариант: первый найденный GPU,
    // либо CPU, если GPU не нашлось вовсе.
    if let Some(first_gpu) = backends.iter_mut().find(|backend| backend.kind == "gpu") {
        first_gpu.is_default = true;
    } else if let Some(cpu) = backends.first_mut() {
        cpu.is_default = true;
    }

    backends
}

#[cfg(feature = "gpu-backends")]
fn backend_label(backend: wgpu::Backend) -> &'static str {
    match backend {
        wgpu::Backend::Vulkan => "Vulkan",
        wgpu::Backend::Dx12 => "DirectX 12",
        wgpu::Backend::Metal => "Metal",
        wgpu::Backend::Gl => "OpenGL",
        _ => "Неизвестный backend",
    }
}

/// Заглушка на случай, если фича `gpu-backends` временно выключена
/// (например, при первой сборке до добавления `wgpu` в Cargo.toml) —
/// команда остаётся зарегистрированной, чтобы фронтенд не падал, но
/// честно возвращает только CPU.
#[cfg(not(feature = "gpu-backends"))]
#[tauri::command]
fn list_render_backends() -> Vec<RenderBackendInfo> {
    vec![RenderBackendInfo {
        id: "cpu".into(),
        label: "CPU (универсальный, всегда доступен)".into(),
        kind: "cpu".into(),
        is_default: true,
    }]
}

// ============================================================================
// TAURI-КОМАНДЫ: КАМЕРА
// ============================================================================

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CameraDeviceInfo {
    index: u32,
    name: String,
    description: String,
    /// Эвристика по имени устройства ("OBS Virtual Camera" и т.п.) — так
    /// UI может показать виртуальную камеру OBS отдельным пунктом
    /// "Video Source → OBS", хотя технически это та же камера.
    is_probably_obs_virtual_camera: bool,
}

#[cfg(feature = "camera")]
#[tauri::command]
fn list_camera_devices() -> Result<Vec<CameraDeviceInfo>, String> {
    let devices =
        nokhwa::query(nokhwa::utils::ApiBackend::Auto).map_err(|error| error.to_string())?;
    Ok(devices
        .into_iter()
        .enumerate()
        .map(|(index, device)| {
            let name = device.human_name();
            let is_obs = name.to_lowercase().contains("obs");
            CameraDeviceInfo {
                index: index as u32,
                name: name.clone(),
                description: device.description().to_string(),
                is_probably_obs_virtual_camera: is_obs,
            }
        })
        .collect())
}

/// Захватывает один кадр с камеры и возвращает его как base64 PNG. Кадр
/// затем прогоняется через `convert_frame_to_ascii` — тем же движком, что
/// и обычные изображения.
#[cfg(feature = "camera")]
#[tauri::command]
fn capture_camera_frame(device_index: u32) -> Result<String, String> {
    use nokhwa::pixel_format::RgbAFormat;
    use nokhwa::utils::{CameraIndex, RequestedFormat, RequestedFormatType};
    use nokhwa::Camera;

    let index = CameraIndex::Index(device_index);
    let format = RequestedFormat::new::<RgbAFormat>(RequestedFormatType::AbsoluteHighestResolution);
    let mut camera = Camera::new(index, format).map_err(|error| error.to_string())?;
    camera.open_stream().map_err(|error| error.to_string())?;
    let frame = camera.frame().map_err(|error| error.to_string())?;
    let decoded = frame
        .decode_image::<RgbAFormat>()
        .map_err(|error| error.to_string())?;
    let _ = camera.stop_stream();

    encode_rgba_as_base64_png(decoded)
}

#[cfg(not(feature = "camera"))]
#[tauri::command]
fn list_camera_devices() -> Result<Vec<CameraDeviceInfo>, String> {
    Err("Поддержка камеры не собрана в этой сборке (фича \"camera\" выключена)".into())
}

#[cfg(not(feature = "camera"))]
#[tauri::command]
fn capture_camera_frame(_device_index: u32) -> Result<String, String> {
    Err("Поддержка камеры не собрана в этой сборке (фича \"camera\" выключена)".into())
}

// ============================================================================
// TAURI-КОМАНДЫ: ЗАХВАТ ЭКРАНА
// ============================================================================

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ScreenSourceInfo {
    id: String,
    label: String,
    width: u32,
    height: u32,
    kind: String, // "monitor" | "window"
}

#[cfg(feature = "screen")]
#[tauri::command]
fn list_screen_sources() -> Result<Vec<ScreenSourceInfo>, String> {
    let mut sources = Vec::new();

    let monitors = xcap::Monitor::all().map_err(|error| error.to_string())?;
    for monitor in monitors {
        sources.push(ScreenSourceInfo {
            id: format!("monitor:{}", monitor.id().map_err(|e| e.to_string())?),
            label: monitor.name().map_err(|e| e.to_string())?,
            width: monitor.width().map_err(|e| e.to_string())?,
            height: monitor.height().map_err(|e| e.to_string())?,
            kind: "monitor".into(),
        });
    }

    // Отдельные окна перечисляем тоже — пользователь может захотеть
    // конвертировать конкретное приложение, а не весь монитор целиком.
    if let Ok(windows) = xcap::Window::all() {
        for window in windows {
            if window.is_minimized().map_err(|e| e.to_string())? {
                continue;
            }
            sources.push(ScreenSourceInfo {
                id: format!("window:{}", window.id().map_err(|e| e.to_string())?),
                label: window.title().map_err(|e| e.to_string())?,
                width: window.width().map_err(|e| e.to_string())?,
                height: window.height().map_err(|e| e.to_string())?,
                kind: "window".into(),
            });
        }
    }

    Ok(sources)
}

#[cfg(feature = "screen")]
#[tauri::command]
fn capture_screen_frame(source_id: String) -> Result<String, String> {
    let captured = if let Some(id) = source_id.strip_prefix("monitor:") {
        let monitor_id: u32 = id
            .parse()
            .map_err(|_| "Некорректный id монитора".to_string())?;
        let monitor = xcap::Monitor::all()
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|monitor| monitor.id().ok() == Some(monitor_id))
            .ok_or_else(|| "Монитор не найден".to_string())?;
        monitor.capture_image().map_err(|error| error.to_string())?
    } else if let Some(id) = source_id.strip_prefix("window:") {
        let window_id: u32 = id.parse().map_err(|_| "Некорректный id окна".to_string())?;
        let window = xcap::Window::all()
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|window| window.id().ok() == Some(window_id))
            .ok_or_else(|| "Окно не найдено".to_string())?;
        window.capture_image().map_err(|error| error.to_string())?
    } else {
        return Err("Неизвестный источник экрана".into());
    };

    encode_rgba_as_base64_png(captured)
}

#[cfg(not(feature = "screen"))]
#[tauri::command]
fn list_screen_sources() -> Result<Vec<ScreenSourceInfo>, String> {
    Err("Поддержка захвата экрана не собрана в этой сборке (фича \"screen\" выключена)".into())
}

#[cfg(not(feature = "screen"))]
#[tauri::command]
fn capture_screen_frame(_source_id: String) -> Result<String, String> {
    Err("Поддержка захвата экрана не собрана в этой сборке (фича \"screen\" выключена)".into())
}

/// Кодирует RGBA-буфер (кадр камеры/экрана) в PNG и base64 — общий шаг для
/// обоих источников, чтобы не дублировать кодирование в двух местах.
#[cfg(any(feature = "camera", feature = "screen"))]
fn encode_rgba_as_base64_png(
    buffer: image::ImageBuffer<image::Rgba<u8>, Vec<u8>>,
) -> Result<String, String> {
    let dynamic = DynamicImage::ImageRgba8(buffer);
    let mut bytes = Vec::new();
    dynamic
        .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
        .map_err(|error| error.to_string())?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
}

// ============================================================================
// TAURI-КОМАНДЫ: ИСТОРИЯ / ЭКСПОРТ
// ============================================================================

fn state_path(app: &AppHandle) -> Result<PathBuf, String> {
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    Ok(directory.join("app_state.json"))
}

/// Загружает персистентное состояние (историю, пользовательские наборы
/// символов, последние настройки). Форма истории намеренно оставлена как
/// свободный JSON (`Value`), а не жёсткая Rust-структура — она полностью
/// описывается и версионируется на стороне фронтенда (следующий файл в
/// очереди), а бэкенд лишь надёжно сохраняет/читает то, что ему прислали,
/// и подчищает записи, чьи файлы уже не существуют на диске.
#[tauri::command]
fn load_app_state(app: AppHandle) -> Result<Value, String> {
    let path = state_path(&app)?;
    if !path.exists() {
        return Ok(serde_json::json!({ "history": [], "customCharacters": [] }));
    }
    let contents = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let mut state: Value = serde_json::from_str(&contents).map_err(|error| error.to_string())?;
    if let Some(history) = state["history"].as_array_mut() {
        history.retain(|item| {
            item["path"]
                .as_str()
                .map(|path| std::path::Path::new(path).exists())
                .unwrap_or(false)
        });
    }
    Ok(state)
}

#[tauri::command]
fn save_app_state(app: AppHandle, state: Value) -> Result<(), String> {
    let path = state_path(&app)?;
    let contents = serde_json::to_string_pretty(&state).map_err(|error| error.to_string())?;
    fs::write(path, contents).map_err(|error| error.to_string())
}

#[tauri::command]
fn clear_history(app: AppHandle) -> Result<(), String> {
    let path = state_path(&app)?;
    let mut state = if path.exists() {
        let contents = fs::read_to_string(&path).map_err(|error| error.to_string())?;
        serde_json::from_str::<Value>(&contents).map_err(|error| error.to_string())?
    } else {
        serde_json::json!({})
    };
    state["history"] = serde_json::json!([]);
    save_app_state(app, state)
}

#[tauri::command]
fn save_export(path: String, content: String, binary: bool) -> Result<(), String> {
    let bytes = if binary {
        let encoded = content
            .split_once(',')
            .map(|(_, value)| value)
            .ok_or_else(|| "Invalid image data".to_string())?;
        base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|error| error.to_string())?
    } else {
        content.into_bytes()
    };
    fs::write(path, bytes).map_err(|error| error.to_string())
}

// ============================================================================
// ТОЧКА ВХОДА
// ============================================================================

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(JobRegistry::new())
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                update(handle).await.unwrap();
            });
            Ok(())
        })
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            load_app_state,
            save_app_state,
            clear_history,
            convert_image_to_ascii,
            convert_frame_to_ascii,
            find_best_settings,
            begin_conversion,
            cancel_conversion,
            begin_settings_search,
            cancel_settings_search,
            begin_job,
            cancel_job,
            save_export,
            read_image_as_base64,
            list_render_backends,
            list_camera_devices,
            capture_camera_frame,
            list_screen_sources,
            capture_screen_frame
        ])
        .run(tauri::generate_context!())
        .expect("Произошла ошибка при запуске приложения");
}

async fn update(app: tauri::AppHandle) -> tauri_plugin_updater::Result<()> {
    if let Some(update) = app.updater()?.check().await? {
        let mut downloaded = 0;

        update
            .download_and_install(
                |chunk_length, content_length| {
                    downloaded += chunk_length;
                    println!("Загружено {downloaded} из {content_length:?}");
                },
                || {
                    println!("Загрузка завершена");
                },
            )
            .await?;

        println!("Обновление установлено");
        app.restart();
    }

    Ok(())
}

// ============================================================================
// ТЕСТЫ
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversion_result_text_is_row_major() {
        let values = ["a", "b", "c", "d", "e", "f"]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>();
        assert_eq!(values_to_text(&values, 3), "abc\ndef");
    }

    #[test]
    fn settings_resolution_is_bounded() {
        assert_eq!(best_resolution(1), 64);
        assert_eq!(best_resolution(4_000), 180);
        assert_eq!(best_resolution(800), 100);
    }

    #[test]
    fn percentile_ignores_single_pixel_outliers() {
        // 1000 пикселей в узком диапазоне 100..110 плюс один шумный пиксель
        // на 255 — 98-й перцентиль не должен "прыгнуть" на 255.
        let mut histogram = [0u32; 256];
        for level in 100..110 {
            histogram[level] = 100;
        }
        histogram[255] = 1;
        let p_high = percentile_from_histogram(&histogram, 0.98);
        assert!(
            p_high < 200,
            "единичный выброс не должен растягивать диапазон: {p_high}"
        );
    }

    #[test]
    fn brightness_contrast_roundtrip_at_neutral_settings() {
        // При brightness=0, contrast=0, invert=false функция не должна
        // менять значение (с точностью до clamp).
        let value = apply_brightness_contrast(0.42, 0, 0, false);
        assert!((value - 0.42).abs() < 1e-5);
    }

    #[test]
    fn brightness_contrast_inverts_when_requested() {
        let value = apply_brightness_contrast(0.2, 0, 0, true);
        assert!((value - 0.8).abs() < 1e-5);
    }

    #[test]
    fn job_registry_cancels_previous_version() {
        let jobs = JobRegistry::new();
        let first = jobs.begin("conversion");
        assert!(jobs.is_current("conversion", first));
        let second = jobs.begin("conversion");
        assert!(!jobs.is_current("conversion", first));
        assert!(jobs.is_current("conversion", second));
    }
}
