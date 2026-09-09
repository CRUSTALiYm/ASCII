#[path = "lib/adaptive.rs"]
mod adaptive;
#[path = "lib/camera.rs"]
mod camera;
#[path = "lib/compute/mod.rs"]
mod compute;
#[path = "lib/engine.rs"]
mod engine;
#[path = "lib/jobs.rs"]
mod jobs;
#[path = "lib/params.rs"]
mod params;
#[path = "lib/render_backends.rs"]
mod render_backends;
#[path = "lib/screen.rs"]
mod screen;
#[path = "lib/storage.rs"]
mod storage;

use jobs::JobRegistry;
use tauri_plugin_updater::UpdaterExt;

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
            storage::load_app_state,
            storage::save_app_state,
            storage::clear_history,
            storage::save_export,
            storage::read_image_as_base64,
            engine::convert_image_to_ascii,
            engine::convert_frame_to_ascii,
            engine::find_best_settings,
            engine::begin_conversion,
            engine::cancel_conversion,
            engine::begin_settings_search,
            engine::cancel_settings_search,
            jobs::begin_job,
            jobs::cancel_job,
            render_backends::list_render_backends,
            compute::list_compute_backends,
            camera::list_camera_devices,
            camera::capture_camera_frame,
            screen::list_screen_sources,
            screen::capture_screen_frame
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
