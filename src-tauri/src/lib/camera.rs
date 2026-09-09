use base64::Engine;
use image::DynamicImage;
use serde::Serialize;
use std::io::Cursor;

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CameraDeviceInfo {
    index: u32,
    name: String,
    description: String,
    is_probably_obs_virtual_camera: bool,
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
pub fn capture_camera_frame(device_index: u32) -> Result<String, String> {
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
pub fn list_camera_devices() -> Result<Vec<CameraDeviceInfo>, String> {
    Err("Поддержка камеры не собрана в этой сборке (фича \"camera\" выключена)".into())
}

#[cfg(not(feature = "camera"))]
#[tauri::command]
pub fn capture_camera_frame(_device_index: u32) -> Result<String, String> {
    Err("Поддержка камеры не собрана в этой сборке (фича \"camera\" выключена)".into())
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