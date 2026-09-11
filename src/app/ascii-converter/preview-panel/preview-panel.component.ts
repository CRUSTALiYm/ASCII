import {
  AfterViewInit,
  Component,
  ElementRef,
  OnDestroy,
  ViewChild,
  effect,
  input,
  signal,
} from "@angular/core";

import { ConvertResult } from "../../core/models/convert-result.model";
import {
  Rect,
  fitContain,
  pointToFraction,
} from "../../core/utils/coordinate-mapper";
import {
  renderAsciiToCanvas,
  resultAspectRatio,
} from "../../core/utils/ascii-canvas-render";

export type PreviewMode = "result" | "original" | "overlay" | "slider";

const EMPTY_RECT: Rect = { x: 0, y: 0, width: 0, height: 0 };

@Component({
  selector: "app-preview-panel",
  templateUrl: "./preview-panel.component.html",
  styleUrl: "./preview-panel.component.scss",
})
export class PreviewPanelComponent implements AfterViewInit, OnDestroy {
  readonly result = input<ConvertResult | null>(null);
  readonly originalSrc = input<string | null>(null);
  readonly tone = input("#e5e7eb");

  @ViewChild("stage") private stageRef?: ElementRef<HTMLDivElement>;
  @ViewChild("canvas") private canvasRef?: ElementRef<HTMLCanvasElement>;

  readonly mode = signal<PreviewMode>("result");
  readonly sliderPosition = signal(0.5);
  readonly resultRect = signal<Rect>(EMPTY_RECT);
  readonly originalRect = signal<Rect>(EMPTY_RECT);

  private resizeObserver?: ResizeObserver;
  private originalNaturalSize: { width: number; height: number } | null =
    null;

  constructor() {
    // Раскладка (fitContain) — реагирует на смену результата/режима и на
    // ресайз панели (см. ngAfterViewInit).
    effect(() => {
      this.result();
      this.mode();
      this.recomputeLayout();
    });

    // Рендер canvas — единственный вызов renderAsciiToCanvas на весь
    // компонент, тот же самый и для Result, и для Overlay, и для Slider.
    effect(() => {
      const result = this.result();
      const tone = this.tone();
      const rect = this.resultRect();
      const canvas = this.canvasRef?.nativeElement;
      if (!result || !canvas || rect.width <= 0) return;

      const dpr = window.devicePixelRatio || 1;
      canvas.width = Math.max(1, Math.round(rect.width * dpr));
      canvas.height = Math.max(1, Math.round(rect.height * dpr));
      renderAsciiToCanvas(canvas, result, { color: tone });
    });
  }

  ngAfterViewInit(): void {
    if (!this.stageRef) return;
    this.resizeObserver = new ResizeObserver(() => this.recomputeLayout());
    this.resizeObserver.observe(this.stageRef.nativeElement);
  }

  ngOnDestroy(): void {
    this.resizeObserver?.disconnect();
  }

  setMode(mode: PreviewMode): void {
    this.mode.set(mode);
  }

  onOriginalImageLoad(image: HTMLImageElement): void {
    this.originalNaturalSize = {
      width: image.naturalWidth,
      height: image.naturalHeight,
    };
    this.recomputeLayout();
  }

  onSliderPointerDown(event: PointerEvent): void {
    (event.target as HTMLElement).setPointerCapture(event.pointerId);
    this.updateSliderFromEvent(event);
  }

  onSliderPointerMove(event: PointerEvent): void {
    if (event.buttons === 0) return;
    this.updateSliderFromEvent(event);
  }

  /** export.service рисует PNG-экспорт из этого же canvas, что видел
   * пользователь — не из отдельного рендера. */
  getCanvas(): HTMLCanvasElement | null {
    return this.canvasRef?.nativeElement ?? null;
  }

  /** В Result/Overlay/Slider оба слоя используют ОДИН и тот же
   * прямоугольник (resultRect), иначе сравнение "плывёт" при несовпадении
   * aspect ratio исходника и ASCII-сетки. Только чистый "Оригинал"
   * показывается в своих собственных пропорциях. */
  activeOriginalRect(): Rect {
    return this.mode() === "original" ? this.originalRect() : this.resultRect();
  }

  originalClipPath(): string | null {
    if (this.mode() !== "slider") return null;
    const visiblePercent = this.sliderPosition() * 100;
    return `inset(0 ${100 - visiblePercent}% 0 0)`;
  }

  resultClipPath(): string | null {
    if (this.mode() !== "slider") return null;
    const hiddenPercent = this.sliderPosition() * 100;
    return `inset(0 0 0 ${hiddenPercent}%)`;
  }

  private updateSliderFromEvent(event: PointerEvent): void {
    const stage = this.stageRef?.nativeElement;
    if (!stage) return;
    const bounds = stage.getBoundingClientRect();
    this.sliderPosition.set(
      pointToFraction(event.clientX - bounds.left, this.resultRect()),
    );
  }

  private recomputeLayout(): void {
    const stage = this.stageRef?.nativeElement;
    const result = this.result();
    if (!stage) return;

    const container = {
      width: stage.clientWidth,
      height: stage.clientHeight,
    };

    if (result) {
      const aspect = resultAspectRatio(result);
      this.resultRect.set(fitContain({ width: aspect, height: 1 }, container));
    }
    if (this.originalNaturalSize) {
      this.originalRect.set(fitContain(this.originalNaturalSize, container));
    }
  }
}