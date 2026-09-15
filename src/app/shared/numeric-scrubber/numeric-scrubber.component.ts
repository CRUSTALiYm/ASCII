import {
  Component,
  ElementRef,
  EventEmitter,
  Input,
  Output,
  ViewChild,
  signal,
} from "@angular/core";

@Component({
  selector: "app-numeric-scrubber",
  templateUrl: "./numeric-scrubber.component.html",
  styleUrl: "./numeric-scrubber.component.scss",
})
export class NumericScrubberComponent {
  @Input() label = "";
  @Input() value = 0;
  @Input() min?: number;
  @Input() max?: number;
  @Input() step = 1;
  /** Пикселей перетаскивания на один шаг — больше значение, мягче скраб. */
  @Input() sensitivity = 4;
  @Output() readonly valueChange = new EventEmitter<number>();

  @ViewChild("input") private inputRef?: ElementRef<HTMLInputElement>;

  readonly editing = signal(false);
  readonly draftValue = signal("");

  private dragStartX = 0;
  private dragStartValue = 0;
  private dragged = false;

  get displayValue(): string {
    return this.formatValue(this.value);
  }

  onPointerDown(event: PointerEvent): void {
    if (this.editing()) return;
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    this.dragStartX = event.clientX;
    this.dragStartValue = this.value;
    this.dragged = false;
  }

  onPointerMove(event: PointerEvent): void {
    if (event.buttons === 0 || this.editing()) return;
    const deltaX = event.clientX - this.dragStartX;
    if (!this.dragged && Math.abs(deltaX) < 3) return;
    this.dragged = true;

    const deltaSteps = Math.trunc(deltaX / this.sensitivity);
    const next = this.clamp(this.dragStartValue + deltaSteps * this.step);
    if (next !== this.value) this.valueChange.emit(next);
  }

  onPointerUp(): void {
    if (!this.dragged) this.startEditing();
    this.dragged = false;
  }

  startEditing(): void {
    this.draftValue.set(String(this.value));
    this.editing.set(true);
    queueMicrotask(() => this.inputRef?.nativeElement.focus());
  }

  /** Ничего не клэмпим на каждое нажатие клавиши — иначе первая же цифра
   * меньше min откатывает поле назад и не даёт набрать число целиком
   * (например "2" из "200" сразу превращалось бы в min). */
  onDraftInput(raw: string): void {
    this.draftValue.set(raw);
  }

  commitDraft(): void {
    const parsed = Number(this.draftValue());
    this.editing.set(false);
    if (!Number.isFinite(parsed)) return;
    const next = this.clamp(parsed);
    if (next !== this.value) this.valueChange.emit(next);
  }

  onKeydown(event: KeyboardEvent): void {
    if (event.key === "Enter") {
      (event.target as HTMLElement).blur();
    } else if (event.key === "Escape") {
      this.editing.set(false);
    }
  }

  private clamp(value: number): number {
    let result = value;
    if (this.min !== undefined) result = Math.max(this.min, result);
    if (this.max !== undefined) result = Math.min(this.max, result);
    return Math.round(result * 100) / 100;
  }

  private formatValue(value: number): string {
    return Number.isInteger(value) ? String(value) : value.toFixed(2);
  }
}