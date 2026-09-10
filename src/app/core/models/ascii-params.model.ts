export type ComputeBackendId = "auto" | "cpu" | "cuda" | "webgpu" | "opencl" | "directcompute";

export interface AsciiParams {
  jobId: number;
  columns: number;
  characters: string;

  // "Целевые" правки — финальная стилизация уже готового результата.
  brightness: number;
  contrast: number;
  invert: boolean;

  // "Исходные" правки — коррекция источника до анализа структуры.
  sourceBrightness: number;
  sourceContrast: number;
  sourceInvert: boolean;
  sourceBlackWhite: boolean;
  sourceThreshold: number;

  // Адаптивный анализ яркости (region-tile, см. adaptive.rs).
  adaptive: boolean;
  adaptiveRegionTiles: number;
  adaptivePercentile: number;
  adaptiveSmoothing: number;
  adaptiveMinRange: number;
  adaptiveDespikeThreshold: number;

  computeBackend: ComputeBackendId;
}

/** Те же значения по умолчанию, что serde-дефолты в params.rs.
 * jobId=0 — заглушка: реальный id всегда берётся из begin_conversion
 * непосредственно перед отправкой запроса, здесь он не нужен. */
export function defaultAsciiParams(): AsciiParams {
  return {
    jobId: 0,
    columns: 100,
    characters: " .:-=+*#%@",

    brightness: 0,
    contrast: 0,
    invert: false,

    sourceBrightness: 0,
    sourceContrast: 0,
    sourceInvert: false,
    sourceBlackWhite: false,
    sourceThreshold: 50,

    adaptive: true,
    adaptiveRegionTiles: 8,
    adaptivePercentile: 5,
    adaptiveSmoothing: 0.6,
    adaptiveMinRange: 30,
    adaptiveDespikeThreshold: 0.12,

    computeBackend: "auto",
  };
}

/** Поля, которые реально редактируются в UI — без служебного jobId. */
export type AsciiParamsInput = Omit<AsciiParams, "jobId">;

/** Зеркалит Rust `ConvertRequest` — `AsciiParams` + путь к файлу (flatten). */
export interface ConvertRequest extends AsciiParams {
  path: string;
}

/** Зеркалит Rust `FrameConvertRequest` — `AsciiParams` + кадр как base64 PNG. */
export interface FrameConvertRequest extends AsciiParams {
  frameBase64: string;
}
