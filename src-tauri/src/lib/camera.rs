use base64::Engine;
use image::DynamicImage;
use serde::Serialize;
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::mpsc;
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

#[cfg(feature = "camera")]
type FrameResult =
    Result<image::ImageBuffer<image::Rgba<u8>, Vec<u8>>, String>;

#[cfg(feature = "camera")]
enum CameraCommand {
    CaptureFrame { reply: mpsc::Sender<FrameResult> },
    Stop,
}

/// Хендл на выделенный поток одной камеры. `Sender` сам по себе `Send`/`Sync`
/// независимо от того, что происходит на другом конце — поэтому именно
/// через него, а не через сам `nokhwa::Camera`, реестр может безопасно
/// жить в `tauri::State`.
#[cfg(feature = "camera")]
struct CameraHandle {
    sender: mpsc::Sender<CameraCommand>,
}

/// Держит по одному выделенному потоку на каждую открытую камеру между
/// вызовами `capture_camera_frame`. `nokhwa::Camera` на Windows оборачивает
/// COM-объект (MSMF) — COM-объекты не `Send`, их нельзя передавать между
/// потоками, только вызывать с того потока, где создали. Поэтому камера
/// живёт целиком на своём потоке, а сюда попадает только канал команд.
/// Открывается один раз при первом кадре, живёт до явного
/// `release_camera` — раньше камера переоткрывалась на каждый вызов, что
/// было и медленно, и на части драйверов роняло захват с ошибкой.
#[cfg(feature = "camera")]
pub struct CameraRegistry {
    handles: Mutex<HashMap<u32, CameraHandle>>,
}

#[cfg(feature = "camera")]
impl CameraRegistry {
    pub fn new() -> Self {
        Self {
            handles: Mutex::new(HashMap::new()),
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
    let sender = {
        let mut handles = registry.handles.lock().map_err(|error| error.to_string())?;
        if !handles.contains_key(&device_index) {
            let sender = spawn_camera_worker(device_index)?;
            handles.insert(device_index, CameraHandle { sender });
        }
        handles
            .get(&device_index)
            .expect("only just inserted")
            .sender
            .clone()
    };

    let (reply_tx, reply_rx) = mpsc::channel();
    sender
        .send(CameraCommand::CaptureFrame { reply: reply_tx })
        .map_err(|_| "Поток камеры уже остановлен".to_string())?;

    let buffer = reply_rx
        .recv()
        .map_err(|error| error.to_string())??;

    encode_rgba_as_base64_png(buffer)
}

/// Явно останавливает поток камеры и убирает её из реестра — вызывается
/// при остановке показа/смене источника на фронтенде, чтобы устройство не
/// оставалось занятым приложением, когда превью больше не показывается.
#[cfg(feature = "camera")]
#[tauri::command]
pub fn release_camera(
    device_index: u32,
    registry: tauri::State<CameraRegistry>,
) -> Result<(), String> {
    let mut handles = registry.handles.lock().map_err(|error| error.to_string())?;
    if let Some(handle) = handles.remove(&device_index) {
        let _ = handle.sender.send(CameraCommand::Stop);
    }
    Ok(())
}

/// Поднимает камеру и выделенный под неё поток. Возвращает канал команд
/// только после того, как камера реально открылась (или ошибку, если не
/// открылась) — так первый `capture_camera_frame` не может обогнать
/// инициализацию.
#[cfg(feature = "camera")]
fn spawn_camera_worker(device_index: u32) -> Result<mpsc::Sender<CameraCommand>, String> {
    use nokhwa::pixel_format::RgbAFormat;
    use nokhwa::utils::{CameraIndex, RequestedFormat, RequestedFormatType};
    use nokhwa::Camera;

    let (command_tx, command_rx) = mpsc::channel::<CameraCommand>();
    let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();

    std::thread::spawn(move || {
        let index = CameraIndex::Index(device_index);
        // AbsoluteHighestResolution падал в MSMF-бэкенде с
        // "NotImplementedError: Not available on WASM" — generic-заглушка
        // nokhwa для комбинаций, не реализованных для конкретного бэкенда.
        // AbsoluteHighestFrameRate — вариант из официального примера
        // библиотеки, реально поддержан MSMF.
        let format =
            RequestedFormat::new::<RgbAFormat>(RequestedFormatType::AbsoluteHighestFrameRate);

        let mut camera = match Camera::new(index, format) {
            Ok(camera) => camera,
            Err(error) => {
                let _ = ready_tx.send(Err(error.to_string()));
                return;
            }
        };
        if let Err(error) = camera.open_stream() {
            let _ = ready_tx.send(Err(error.to_string()));
            return;
        }
        let _ = ready_tx.send(Ok(()));

        for command in command_rx {
            match command {
                CameraCommand::CaptureFrame { reply } => {
                    let result = camera
                        .frame()
                        .map_err(|error| error.to_string())
                        .and_then(|frame| {
                            frame
                                .decode_image::<RgbAFormat>()
                                .map_err(|error| error.to_string())
                        });
                    let _ = reply.send(result);
                }
                CameraCommand::Stop => break,
            }
        }
        let _ = camera.stop_stream();
    });

    ready_rx
        .recv()
        .map_err(|error| error.to_string())??;

    Ok(command_tx)
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