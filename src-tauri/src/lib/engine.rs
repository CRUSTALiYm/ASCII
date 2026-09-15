use crate::adaptive::{render_adaptive, render_legacy};
use crate::jobs::JobRegistry;
use crate::params::{
    AsciiParams, BestSettings, ConversionProgress, ConvertRequest, ConvertResult,
    FrameConvertRequest,
};
use base64::Engine;
use image::{DynamicImage, ImageReader};
use tauri::{AppHandle, Emitter, State};

/// Единственное место, где кадр (файл, кадр камеры/экрана) превращается в
/// ASCII-сетку. И `convert_image_to_ascii`, и `convert_frame_to_ascii`
/// вызывают именно её — Preview и Export не могут разойтись на бэкенде.
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

fn best_resolution(width: u32) -> u32 {
    (width / 8).clamp(64, 180)
}

#[tauri::command]
pub fn find_best_settings(
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
pub fn begin_settings_search(jobs: State<JobRegistry>) -> u64 {
    jobs.begin("settings_search")
}

#[tauri::command]
pub fn cancel_settings_search(jobs: State<JobRegistry>) {
    jobs.cancel("settings_search");
}

#[tauri::command]
pub fn begin_conversion(jobs: State<JobRegistry>) -> u64 {
    jobs.begin("conversion")
}

#[tauri::command]
pub fn cancel_conversion(jobs: State<JobRegistry>) {
    jobs.cancel("conversion");
}

#[tauri::command]
pub fn convert_image_to_ascii(
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

/// Тот же движок, но на входе — уже захваченный кадр (base64 PNG) вместо
/// пути к файлу: используется для камеры и захвата экрана.
#[tauri::command]
pub fn convert_frame_to_ascii(
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
}