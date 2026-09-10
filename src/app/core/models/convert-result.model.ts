/** Результат одной конвертации — ответ `convert_image_to_ascii`/`convert_frame_to_ascii`. */
export interface ConvertResult {
  columns: number;
  rows: number;
  /** Символы построчно, row-major, длина = columns * rows. */
  values: string[];
  /** Тот же результат, уже собранный в текст с переносами строк. */
  text: string;
}

/** Событие "conversion-progress" (см. app.emit в engine.rs). */
export interface ConversionProgress {
  jobId: number;
  processed: number;
  total: number;
}

/** Ответ `find_best_settings`. */
export interface BestSettings {
  resolution: number;
}