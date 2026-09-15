/** Зеркалит Rust `CameraDeviceInfo` (src-tauri/src/lib/camera.rs) —
 * элемент ответа команды `list_camera_devices`. */
export interface CameraDeviceInfo {
  index: number;
  name: string;
  description: string;
  isProbablyObsVirtualCamera: boolean;
}

/** Зеркалит Rust `ScreenSourceInfo` (src-tauri/src/lib/screen.rs) —
 * элемент ответа команды `list_screen_sources`. */
export interface ScreenSourceInfo {
  id: string;
  label: string;
  width: number;
  height: number;
  kind: "monitor" | "window";
}

/** Чисто фронтендовое понятие — на бэкенде такого enum нет, это то, что
 * пользователь выбрал в source-picker. */
export type VideoSourceKind = "file" | "camera" | "screen";