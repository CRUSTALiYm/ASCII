import { ComputeBackendId } from "./ascii-params.model";

/** Зеркалит Rust `ComputeBackendInfo` (src-tauri/src/lib/compute/mod.rs) —
 * ответ команды `list_compute_backends`. */
export interface ComputeBackendInfo {
  id: ComputeBackendId;
  label: string;
  /** Реально обнаружено на этой системе (или "cpu" — всегда true). */
  isAvailable: boolean;
  /** В движке для этого бэкенда подключён реальный расчёт на GPU, а не
   * только детект (сейчас — cuda/webgpu/opencl; directcompute — нет). */
  executesOnGpu: boolean;
  isDefault: boolean;
}

/** Зеркалит Rust `RenderBackendInfo` (src-tauri/src/lib/render_backends.rs) —
 * ответ команды `list_render_backends`. Отдельная сущность от
 * `ComputeBackendInfo`: это про рендер (Vulkan/DX12/Metal/GL для показа
 * кадра), не про сам расчёт ASCII. */
export interface RenderBackendInfo {
  id: string;
  label: string;
  kind: "cpu" | "gpu";
  isDefault: boolean;
}
