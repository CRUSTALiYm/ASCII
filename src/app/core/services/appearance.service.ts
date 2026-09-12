import { Injectable, signal } from "@angular/core";

export interface TonePreset {
  label: string;
  value: string; // CSS-цвет
}

export function builtInTonePresets(): TonePreset[] {
  return [
    { label: "Белый", value: "#e5e7eb" },
    { label: "Зелёный терминал", value: "#33ff66" },
    { label: "Янтарный", value: "#ffb347" },
    { label: "Голубой", value: "#38bdf8" },
  ];
}

export type PanelPosition = "left" | "right";

/** Цвет текста превью и фон — чисто визуальные настройки, в `AsciiParams`
 * их нет, на бэкенд не уходят. */
@Injectable({ providedIn: "root" })
export class AppearanceService {
  private readonly _tone = signal<string>(builtInTonePresets()[0].value);
  readonly tone = this._tone.asReadonly();

  /** `null` — прозрачный фон (по умолчанию). Иначе — CSS-цвет из колорпикера. */
  private readonly _background = signal<string | null>(null);
  readonly background = this._background.asReadonly();

  private readonly _panelPosition = signal<PanelPosition>("right");
  readonly panelPosition = this._panelPosition.asReadonly();

  setTone(value: string): void {
    this._tone.set(value);
  }

  restoreTone(value: string | undefined): void {
    if (value) this._tone.set(value);
  }

  setBackground(value: string | null): void {
    this._background.set(value);
  }

  restoreBackground(value: string | null | undefined): void {
    if (value !== undefined) this._background.set(value);
  }

  setPanelPosition(value: PanelPosition): void {
    this._panelPosition.set(value);
  }

  togglePanelPosition(): void {
    this._panelPosition.update((current) => (current === "right" ? "left" : "right"));
  }

  restorePanelPosition(value: PanelPosition | undefined): void {
    if (value) this._panelPosition.set(value);
  }
}