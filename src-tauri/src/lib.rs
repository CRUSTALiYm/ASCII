use base64::Engine;
use image::imageops::FilterType;
use image::ImageReader;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{AppHandle, Emitter, Manager};

static CONVERSION_JOB: AtomicU64 = AtomicU64::new(0);
static SETTINGS_JOB: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConvertRequest {
    job_id: u64,
    path: String,
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
}

#[derive(Debug, Serialize)]
struct ConvertResult {
    columns: u32,
    rows: u32,
    values: Vec<String>,
}

#[derive(Debug, Serialize)]
struct BestSettings {
    resolution: u32,
}

#[tauri::command]
fn find_best_settings(path: String, job_id: u64) -> Result<BestSettings, String> {
    if SETTINGS_JOB.load(Ordering::Relaxed) != job_id {
        return Err("settings_search_cancelled".into());
    }
    let image = ImageReader::open(path)
        .map_err(|error| error.to_string())?
        .decode()
        .map_err(|error| error.to_string())?;
    if SETTINGS_JOB.load(Ordering::Relaxed) != job_id {
        return Err("settings_search_cancelled".into());
    }
    Ok(BestSettings {
        resolution: (image.width() / 8).clamp(64, 180),
    })
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ConversionProgress {
    job_id: u64,
    processed: usize,
    total: usize,
}

#[tauri::command]
fn convert_image_to_ascii(
    app: AppHandle,
    request: ConvertRequest,
) -> Result<ConvertResult, String> {
    let image = ImageReader::open(&request.path)
        .map_err(|error| error.to_string())?
        .decode()
        .map_err(|error| error.to_string())?;
    let columns = request.columns.max(24);
    let ratio = image.height() as f32 / image.width().max(1) as f32;
    let rows = ((columns as f32 * ratio * 0.5).round() as u32).max(8);
    let resized = image
        .resize_exact(columns, rows, FilterType::Triangle)
        .to_luma8();
    let levels = request.characters.chars().collect::<Vec<_>>();
    if levels.is_empty() {
        return Err("Character set cannot be empty".into());
    }

    let pixels = resized.pixels().copied().collect::<Vec<_>>();
    let total = pixels.len();
    let processed = AtomicU64::new(0);
    let step = ((total / 100).max(1)) as u64;
    let values = pixels
        .par_iter()
        .map(|pixel| {
            if CONVERSION_JOB.load(Ordering::Relaxed) != request.job_id {
                return Err("conversion_cancelled".to_string());
            }

            let mut source_value = (pixel[0] as f32 / 255.0 - 0.5)
                * (1.0 + request.source_contrast as f32 / 100.0)
                + 0.5
                + request.source_brightness as f32 / 100.0;
            source_value = source_value.clamp(0.0, 1.0);
            if request.source_invert {
                source_value = 1.0 - source_value;
            }
            if request.source_black_white && request.source_threshold > 0 {
                source_value = if source_value * 100.0 < request.source_threshold as f32 {
                    0.0
                } else {
                    1.0
                };
            }

            let mut value = (source_value - 0.5) * (1.0 + request.contrast as f32 / 100.0)
                + 0.5
                + request.brightness as f32 / 100.0;
            value = value.clamp(0.0, 1.0);
            if request.invert {
                value = 1.0 - value;
            }
            let index = ((value * levels.len() as f32) as usize).min(levels.len() - 1);
            let current = processed.fetch_add(1, Ordering::Relaxed) + 1;
            if current == total as u64 || current % step == 0 {
                let _ = app.emit(
                    "conversion-progress",
                    ConversionProgress {
                        job_id: request.job_id,
                        processed: current as usize,
                        total,
                    },
                );
            }
            Ok(levels[index].to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(ConvertResult {
        columns,
        rows,
        values,
    })
}

#[tauri::command]
fn begin_conversion() -> u64 {
    CONVERSION_JOB.fetch_add(1, Ordering::Relaxed) + 1
}

#[tauri::command]
fn cancel_conversion() {
    CONVERSION_JOB.fetch_add(1, Ordering::Relaxed);
}

#[tauri::command]
fn begin_settings_search() -> u64 {
    SETTINGS_JOB.fetch_add(1, Ordering::Relaxed) + 1
}

#[tauri::command]
fn cancel_settings_search() {
    SETTINGS_JOB.fetch_add(1, Ordering::Relaxed);
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
            .split_once(",")
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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            load_app_state,
            save_app_state,
            clear_history,
            convert_image_to_ascii,
            find_best_settings,
            begin_conversion,
            cancel_conversion,
            begin_settings_search,
            cancel_settings_search,
            save_export
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
