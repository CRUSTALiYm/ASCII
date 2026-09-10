export interface Size {
  width: number;
  height: number;
}

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** Вписывает `source` в `container` с сохранением соотношения сторон
 * (letterbox/pillarbox). Единая функция для всех режимов нижней панели
 * (Result/Original/Overlay/Slider) — они не считают каждый по-своему и не
 * расходятся между собой на разных разрешениях/aspect ratio. */
export function fitContain(source: Size, container: Size): Rect {
  if (
    source.width <= 0 ||
    source.height <= 0 ||
    container.width <= 0 ||
    container.height <= 0
  ) {
    return { x: 0, y: 0, width: 0, height: 0 };
  }

  const sourceRatio = source.width / source.height;
  const containerRatio = container.width / container.height;

  let width: number;
  let height: number;
  if (sourceRatio > containerRatio) {
    // Источник шире контейнера — упираемся в ширину, поля сверху/снизу.
    width = container.width;
    height = width / sourceRatio;
  } else {
    height = container.height;
    width = height * sourceRatio;
  }

  return {
    x: (container.width - width) / 2,
    y: (container.height - height) / 2,
    width,
    height,
  };
}

/** Точка в координатах контейнера → доля 0..1 по ширине уже вписанного
 * изображения. Нужна для slider-сравнения: без вычитания letterbox-полей
 * ручка "плывёт" при разных aspect ratio источника и панели. */
export function pointToFraction(pointX: number, displayRect: Rect): number {
  if (displayRect.width <= 0) return 0;
  const relative = (pointX - displayRect.x) / displayRect.width;
  return Math.min(1, Math.max(0, relative));
}

/** Точка в координатах контейнера → координаты в исходном изображении.
 * `null`, если точка вне отображаемой области (попала в letterbox-поле). */
export function containerPointToSource(
  point: { x: number; y: number },
  source: Size,
  displayRect: Rect,
): { x: number; y: number } | null {
  if (displayRect.width <= 0 || displayRect.height <= 0) return null;

  const fx = (point.x - displayRect.x) / displayRect.width;
  const fy = (point.y - displayRect.y) / displayRect.height;
  if (fx < 0 || fx > 1 || fy < 0 || fy > 1) return null;

  return { x: fx * source.width, y: fy * source.height };
}