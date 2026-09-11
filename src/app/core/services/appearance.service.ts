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

/** Цвет текста превью — чисто визуальная настройка, в `AsciiParams` её
 * нет, на бэкенд не уходит. */
@Injectable({ providedIn: "root" })
export class AppearanceService {
  private readonly _tone = signal<string>(builtInTonePresets()[0].value);
  readonly tone = this._tone.asReadonly();

  private readonly _panelPosition = signal<PanelPosition>("right");
  readonly panelPosition = this._panelPosition.asReadonly();

  setTone(value: string): void {
    this._tone.set(value);
  }

  restoreTone(value: string | undefined): void {
    if (value) this._tone.set(value);
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