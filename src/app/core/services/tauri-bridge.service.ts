import { Injectable } from "@angular/core";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";

import { AppState } from "../models/app-state.model";
import {
  ConvertRequest,
  FrameConvertRequest,
} from "../models/ascii-params.model";
import {
  BestSettings,
  ConversionProgress,
  ConvertResult,
} from "../models/convert-result.model";
import {
  ComputeBackendInfo,
  RenderBackendInfo,
} from "../models/backend-info.model";
import {
  CameraDeviceInfo,
  ScreenSourceInfo,
} from "../models/capture-source.model";

/** Единственное место в приложении, которое вызывает `invoke`/`listen`.
 * Остальной код Rust-команды напрямую не трогает — только через методы
 * этого сервиса, по одному на каждую команду из lib.rs. */
@Injectable({ providedIn: "root" })
export class TauriBridgeService {
  // --- история / персистентный стейт (storage.rs) --------------------------

  loadAppState(): Promise<AppState> {
    return invoke<AppState>("load_app_state");
  }

  saveAppState(state: AppState): Promise<void> {
    return invoke("save_app_state", { state });
  }

  clearHistory(): Promise<void> {
    return invoke("clear_history");
  }

  saveExport(path: string, content: string, binary: boolean): Promise<void> {
    return invoke("save_export", { path, content, binary });
  }

  readImageAsBase64(path: string): Promise<string> {
    return invoke<string>("read_image_as_base64", { path });
  }

  // --- конвертация (engine.rs) ----------------------------------------------

  beginConversion(): Promise<number> {
    return invoke<number>("begin_conversion");
  }

  cancelConversion(): Promise<void> {
    return invoke("cancel_conversion");
  }

  convertImageToAscii(request: ConvertRequest): Promise<ConvertResult> {
    return invoke<ConvertResult>("convert_image_to_ascii", { request });
  }

  convertFrameToAscii(request: FrameConvertRequest): Promise<ConvertResult> {
    return invoke<ConvertResult>("convert_frame_to_ascii", { request });
  }

  beginSettingsSearch(): Promise<number> {
    return invoke<number>("begin_settings_search");
  }

  cancelSettingsSearch(): Promise<void> {
    return invoke("cancel_settings_search");
  }

  findBestSettings(path: string, jobId: number): Promise<BestSettings> {
    return invoke<BestSettings>("find_best_settings", { path, jobId });
  }

  // --- общий реестр задач, для будущих потоковых источников (jobs.rs) ------

  beginJob(kind: string): Promise<number> {
    return invoke<number>("begin_job", { kind });
  }

  cancelJob(kind: string): Promise<void> {
    return invoke("cancel_job", { kind });
  }

  // --- бэкенды (render_backends.rs, compute/mod.rs) -------------------------

  listRenderBackends(): Promise<RenderBackendInfo[]> {
    return invoke<RenderBackendInfo[]>("list_render_backends");
  }

  listComputeBackends(): Promise<ComputeBackendInfo[]> {
    return invoke<ComputeBackendInfo[]>("list_compute_backends");
  }

  // --- камера (camera.rs) ----------------------------------------------------

  listCameraDevices(): Promise<CameraDeviceInfo[]> {
    return invoke<CameraDeviceInfo[]>("list_camera_devices");
  }

  captureCameraFrame(deviceIndex: number): Promise<string> {
    return invoke<string>("capture_camera_frame", { deviceIndex });
  }

  // --- экран (screen.rs) ------------------------------------------------------

  listScreenSources(): Promise<ScreenSourceInfo[]> {
    return invoke<ScreenSourceInfo[]>("list_screen_sources");
  }

  captureScreenFrame(sourceId: string): Promise<string> {
    return invoke<string>("capture_screen_frame", { sourceId });
  }

  // --- события ------------------------------------------------------------

  onConversionProgress(
    handler: (progress: ConversionProgress) => void,
  ): Promise<UnlistenFn> {
    return listen<ConversionProgress>("conversion-progress", (event) =>
      handler(event.payload),
    );
  }
}