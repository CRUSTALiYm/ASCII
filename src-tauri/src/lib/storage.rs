use base64::Engine;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

fn state_path(app: &AppHandle) -> Result<PathBuf, String> {
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    Ok(directory.join("app_state.json"))
}

#[tauri::command]
pub fn load_app_state(app: AppHandle) -> Result<Value, String> {
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
pub fn save_app_state(app: AppHandle, state: Value) -> Result<(), String> {
    let path = state_path(&app)?;
    let contents = serde_json::to_string_pretty(&state).map_err(|error| error.to_string())?;
    fs::write(path, contents).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn clear_history(app: AppHandle) -> Result<(), String> {
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
pub fn save_export(path: String, content: String, binary: bool) -> Result<(), String> {
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

#[tauri::command]
pub fn read_image_as_base64(path: String) -> Result<String, String> {
    let bytes = std::fs::read(&path).map_err(|error| error.to_string())?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
}
