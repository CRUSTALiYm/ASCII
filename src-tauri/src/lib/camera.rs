use base64::Engine;
use image::DynamicImage;
use serde::Serialize;
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Mutex;

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CameraDeviceInfo {
    index: u32,
    name: String,
    description: String,
    /// Виртуальная камера OBS видна системе как обычная камера — отдельная
    /// интеграция с OBS API не нужна, достаточно выбрать её в списке.
    is_probably_obs_virtual_camera: bool,
}

/// Держит открытые камеры между вызовами `capture_camera_frame`. Раньше
/// каждый вызов заново делал `Camera::new` + `open_stream` + `stop_stream`
/// — это медленно, а на части драйверов быстрое переоткрытие подряд
/// вообще роняло захват с ошибкой. Теперь камера открывается один раз при
/// первом кадре и живёт до явного `release_camera` (вызывается при смене
/// источника/остановке потока на фронтенде).
#[cfg(feature = "camera")]
pub struct CameraRegistry {
    cameras: Mutex<HashMap<u32, nokhwa::Camera>>,
}

#[cfg(feature = "camera")]
impl CameraRegistry {
    pub fn new() -> Self {
        Self {
            cameras: Mutex::new(HashMap::new()),
        }
    }
}

#[cfg(feature = "camera")]
#[tauri::command]
pub fn list_camera_devices() -> Result<Vec<CameraDeviceInfo>, String> {
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
pub fn capture_camera_frame(
    device_index: u32,
    registry: tauri::State<CameraRegistry>,
) -> Result<String, String> {
    use nokhwa::pixel_format::RgbAFormat;
    use nokhwa::utils::{CameraIndex, RequestedFormat, RequestedFormatType};
    use nokhwa::Camera;
    use std::collections::hash_map::Entry;

    let mut cameras = registry.cameras.lock().map_err(|error| error.to_string())?;

    let camera = match cameras.entry(device_index) {
        Entry::Occupied(entry) => entry.into_mut(),
        Entry::Vacant(entry) => {
            let index = CameraIndex::Index(device_index);
            // AbsoluteHighestResolution падал в MSMF-бэкенде с
            // "NotImplementedError: Not available on WASM" — это
            // generic-заглушка nokhwa для нереализованных для конкретного
            // бэкенда комбинаций. AbsoluteHighestFrameRate — вариант из
            // официального примера библиотеки, реально поддержан MSMF.
            let format = RequestedFormat::new::<RgbAFormat>(
                RequestedFormatType::AbsoluteHighestFrameRate,
            );
            let mut camera = Camera::new(index, format).map_err(|error| error.to_string())?;
            camera.open_stream().map_err(|error| error.to_string())?;
            entry.insert(camera)
        }
    };

    let frame = camera.frame().map_err(|error| error.to_string())?;
    let decoded = frame
        .decode_image::<nokhwa::pixel_format::RgbAFormat>()
        .map_err(|error| error.to_string())?;

    encode_rgba_as_base64_png(decoded)
}

/// Явно закрывает и убирает камеру из реестра — вызывается при остановке
/// потока или смене источника на фронтенде, чтобы устройство не оставалось
/// занятым приложением после того, как превью больше не показывается.
#[cfg(feature = "camera")]
#[tauri::command]
pub fn release_camera(device_index: u32, registry: tauri::State<CameraRegistry>) -> Result<(), String> {
    let mut cameras = registry.cameras.lock().map_err(|error| error.to_string())?;
    if let Some(mut camera) = cameras.remove(&device_index) {
        let _ = camera.stop_stream();
    }
    Ok(())
}

#[cfg(not(feature = "camera"))]
#[tauri::command]
pub fn list_camera_devices() -> Result<Vec<CameraDeviceInfo>, String> {
    Err("Поддержка камеры не собрана в этой сборке (фича \"camera\" выключена)".into())
}

#[cfg(not(feature = "camera"))]
#[tauri::command]
pub fn capture_camera_frame(_device_index: u32) -> Result<String, String> {
    Err("Поддержка камеры не собрана в этой сборке (фича \"camera\" выключена)".into())
}

#[cfg(not(feature = "camera"))]
#[tauri::command]
pub fn release_camera(_device_index: u32) -> Result<(), String> {
    Ok(())
}

#[cfg(feature = "camera")]
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