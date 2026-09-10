import { ConvertResult } from "../models/convert-result.model";

export interface AsciiRenderOptions {
  color: string;
  /** Если не задан — фон остаётся прозрачным. */
  background?: string;
  fontFamily?: string;
}

const DEFAULT_FONT_FAMILY =
  '"JetBrains Mono", "Cascadia Code", Consolas, monospace';

/** Рисует `ConvertResult` на canvas как моноширинную сетку символов. Эта
 * функция — единственный рендер во всём приложении: её вызывает и живое
 * превью, и экспорт в PNG, поэтому скачанная картинка пиксель-в-пиксель
 * совпадает с тем, что видел пользователь (а не два разных рендера, как
 * раньше с CSS-фильтром на <img> и отдельным canvas). */
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

/** Соотношение сторон ASCII-сетки — источник для `fitContain` при
 * размещении canvas в панели превью. */
export function resultAspectRatio(result: ConvertResult): number {
  if (result.rows === 0) return 1;
  return result.columns / result.rows;
}