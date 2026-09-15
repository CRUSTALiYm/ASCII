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
  sourceAspectFromResult,
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
  readonly background = input<string | null>(null);

  @ViewChild("stage") private stageRef?: ElementRef<HTMLDivElement>;
  @ViewChild("canvas") private canvasRef?: ElementRef<HTMLCanvasElement>;

  readonly mode = signal<PreviewMode>("result");
  readonly sliderPosition = signal(0.5);

  /** ОДИН прямоугольник для картинки и canvas во всех режимах — раньше у
   * каждого был свой (resultRect считался из columns/rows без поправки на
   * то, что Rust уже сжал rows под моноширинный шрифт), из-за чего
   * Result визуально не совпадал с Original. */
  readonly contentRect = signal<Rect>(EMPTY_RECT);

  private resizeObserver?: ResizeObserver;
  private originalNaturalSize: { width: number; height: number } | null =
    null;

  constructor() {
    effect(() => {
      this.result();
      this.mode();
      this.recomputeLayout();
    });

    effect(() => {
      const result = this.result();
      const tone = this.tone();
      const background = this.background();
      const rect = this.contentRect();
      const canvas = this.canvasRef?.nativeElement;
      if (!result || !canvas || rect.width <= 0) return;

      const dpr = window.devicePixelRatio || 1;
      canvas.width = Math.max(1, Math.round(rect.width * dpr));
      canvas.height = Math.max(1, Math.round(rect.height * dpr));
      renderAsciiToCanvas(canvas, result, { color: tone, background });
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

  /** export.service рисует PNG-экспорт по этим же пропорциям, но в
   * отдельном, полноразмерном canvas — не по маленькому canvas превью. */
  getSourceAspect(): number {
    if (this.originalNaturalSize) {
      return this.originalNaturalSize.width / this.originalNaturalSize.height;
    }
    const result = this.result();
    return result ? sourceAspectFromResult(result) : 1;
  }

  private updateSliderFromEvent(event: PointerEvent): void {
    const stage = this.stageRef?.nativeElement;
    if (!stage) return;
    const bounds = stage.getBoundingClientRect();
    this.sliderPosition.set(
      pointToFraction(event.clientX - bounds.left, this.contentRect()),
    );
  }

  private recomputeLayout(): void {
    const stage = this.stageRef?.nativeElement;
    if (!stage) return;

    const container = {
      width: stage.clientWidth,
      height: stage.clientHeight,
    };

    const sourceSize = this.originalNaturalSize ?? this.approximateSourceSize();
    if (sourceSize) {
      this.contentRect.set(fitContain(sourceSize, container));
    }
  }

  private approximateSourceSize(): { width: number; height: number } | null {
    const result = this.result();
    if (!result) return null;
    return { width: sourceAspectFromResult(result), height: 1 };
  }
}