import { ConvertResult } from "../models/convert-result.model";

export interface AsciiRenderOptions {
  color: string;
  /** `null`/не задан — фон остаётся прозрачным. */
  background?: string | null;
  fontFamily?: string;
}

const DEFAULT_FONT_FAMILY =
  '"JetBrains Mono", "Cascadia Code", Consolas, monospace';

/** Рисует `ConvertResult` на canvas как моноширинную сетку символов. Эта
 * функция — единственный рендер во всём приложении: её вызывает и живое
 * превью, и экспорт (через `createExportCanvas`), поэтому результат
 * пиксель-в-пиксель совпадает с тем, что видел пользователь. */
export function renderAsciiToCanvas(
  canvas: HTMLCanvasElement,
  result: ConvertResult,
  options: AsciiRenderOptions,
): void {
  const context = canvas.getContext("2d");
  if (!context) return;

  const { width, height } = canvas;
  context.clearRect(0, 0, width, height);
  if (options.background) {
    context.fillStyle = options.background;
    context.fillRect(0, 0, width, height);
  }

  if (result.columns <= 0 || result.rows <= 0) return;

  const fontFamily = options.fontFamily ?? DEFAULT_FONT_FAMILY;
  const cellWidth = width / result.columns;
  const cellHeight = height / result.rows;

  // Подбираем размер шрифта по реальной ширине символа в этом шрифте
  // (моноширинные шрифты бывают у́же/шире "среднего"), а не растягиваем
  // строку через fillText-maxWidth — так глифы не искажаются.
  let fontSize = cellHeight;
  context.font = `${fontSize}px ${fontFamily}`;
  const measuredWidth = context.measureText("M").width;
  if (measuredWidth > 0) {
    fontSize *= Math.min(1, cellWidth / measuredWidth);
  }
  context.font = `${fontSize}px ${fontFamily}`;

  context.fillStyle = options.color;
  context.textBaseline = "middle";
  context.textAlign = "left";

  for (let row = 0; row < result.rows; row++) {
    const lineStart = row * result.columns;
    const line = result.values.slice(lineStart, lineStart + result.columns).join("");
    const y = cellHeight * (row + 0.5);
    context.fillText(line, 0, y);
  }
}

// Должно совпадать с `* 0.5` при расчёте rows в engine.rs — это и есть
// компенсация того, что символ моноширинного шрифта уже, чем высок.
const CHAR_HEIGHT_COMPENSATION = 0.5;

/** Пропорции (width/height) исходного изображения, ВОССТАНОВЛЕННЫЕ из
 * размера ASCII-сетки. Использовать только как fallback, когда реальные
 * пропорции оригинала ещё не известны (картинка ещё не загрузилась) —
 * настоящий источник правды это naturalWidth/naturalHeight оригинала,
 * который и должен совпадать с этим значением с точностью до округления.
 *
 * Раньше здесь было голое `columns / rows`, что игнорировало компенсацию
 * из engine.rs и давало пропорции ВДВОЕ шире реальных — отсюда
 * несовпадение Result с Original в превью. */
export function sourceAspectFromResult(result: ConvertResult): number {
  if (result.rows === 0) return 1;
  return result.columns / (result.rows / CHAR_HEIGHT_COMPENSATION);
}

const EXPORT_PIXELS_PER_COLUMN = 16;

/** Рендерит результат в новый, отдельный от превью canvas — с
 * разрешением, привязанным к количеству символов, а не к размеру панели
 * на экране. Экспорт больше не наследует маленькое разрешение живого
 * превью: символы остаются чёткими при увеличении сохранённой картинки. */
export function createExportCanvas(
  result: ConvertResult,
  sourceAspect: number,
  options: AsciiRenderOptions,
): HTMLCanvasElement {
  const canvas = document.createElement("canvas");
  canvas.width = Math.max(1, Math.round(result.columns * EXPORT_PIXELS_PER_COLUMN));
  canvas.height = Math.max(1, Math.round(canvas.width / Math.max(sourceAspect, 0.01)));
  renderAsciiToCanvas(canvas, result, options);
  return canvas;
}