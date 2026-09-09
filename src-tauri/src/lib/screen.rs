use base64::Engine;
use image::DynamicImage;
use serde::Serialize;
use std::io::Cursor;

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ScreenSourceInfo {
    id: String,
    label: String,
    width: u32,
    height: u32,
    kind: String, // "monitor" | "window"
}

#[cfg(feature = "screen")]
#[tauri::command]
pub fn list_screen_sources() -> Result<Vec<ScreenSourceInfo>, String> {
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
pub fn capture_screen_frame(source_id: String) -> Result<String, String> {
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
pub fn list_screen_sources() -> Result<Vec<ScreenSourceInfo>, String> {
    Err("Поддержка захвата экрана не собрана в этой сборке (фича \"screen\" выключена)".into())
}

#[cfg(not(feature = "screen"))]
#[tauri::command]
pub fn capture_screen_frame(_source_id: String) -> Result<String, String> {
    Err("Поддержка захвата экрана не собрана в этой сборке (фича \"screen\" выключена)".into())
}

#[cfg(feature = "screen")]
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
