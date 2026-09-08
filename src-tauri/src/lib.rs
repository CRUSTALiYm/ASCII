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

struct JobRegistry {
    versions: Mutex<HashMap<String, u64>>,
}

impl JobRegistry {
    fn new() -> Self {
        Self {
            versions: Mutex::new(HashMap::new()),
        }
    }

    fn begin(&self, kind: &str) -> u64 {
        let mut map = self.versions.lock().expect("job registry poisoned");
        let next = map.get(kind).copied().unwrap_or(0) + 1;
        map.insert(kind.to_string(), next);
        next
    }

    fn cancel(&self, kind: &str) {
        self.begin(kind);
    }

    fn is_current(&self, kind: &str, id: u64) -> bool {
        let map = self.versions.lock().expect("job registry poisoned");
        map.get(kind).copied() == Some(id)
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct AsciiParams {
    job_id: u64,
    columns: u32,
    characters: String,
    brightness: i32,
    contrast: i32,
    invert: bool,
    source_brightness: i32,
    source_contrast: i32,
    source_invert: bool,
    source_black_white: bool,
    source_threshold: i32,
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

#[derive(Clone, Copy)]
struct CellStats {
    mean: f32,
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

fn compute_global_stats(luma: &image::GrayImage) -> GlobalLumaStats {
    let mut histogram = [0u32; 256];
    for pixel in luma.pixels() {
        histogram[pixel[0] as usize] += 1;
    }
    let p_low = percentile_from_histogram(&histogram, 0.02);
    let p_high = percentile_from_histogram(&histogram, 0.98).max(p_low + 1);
    GlobalLumaStats { p_low, p_high }
}

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

fn apply_brightness_contrast(value: f32, brightness: i32, contrast: i32, invert: bool) -> f32 {
    let mut adjusted =
        (value - 0.5) * (1.0 + contrast as f32 / 100.0) + 0.5 + brightness as f32 / 100.0;
    adjusted = adjusted.clamp(0.0, 1.0);
    if invert {
        adjusted = 1.0 - adjusted;
    }
    adjusted
}

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

trait FrameSource {
    fn next_frame(&mut self) -> Result<Option<DynamicImage>, String>;
    fn frame_rate(&self) -> Option<f32> {
        None
    }
}

struct VideoFileSource;

impl FrameSource for VideoFileSource {
    fn next_frame(&mut self) -> Result<Option<DynamicImage>, String> {
        Err("Декодирование видеофайлов ещё не подключено — см. комментарий у VideoFileSource в lib.rs".into())
    }
}

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

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct RenderBackendInfo {
    id: String,
    label: String,
    kind: String, // "cpu" | "gpu"
    is_default: bool,
}

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

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CameraDeviceInfo {
    index: u32,
    name: String,
    description: String,
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

fn state_path(app: &AppHandle) -> Result<PathBuf, String> {
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    Ok(directory.join("app_state.json"))
}

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
    println!("{:#?}", list_render_backends());

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
