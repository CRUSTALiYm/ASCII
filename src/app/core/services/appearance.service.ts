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

/** Цвет текста превью — чисто визуальная настройка, в `AsciiParams` её
 * нет, на бэкенд не уходит. */
@Injectable({ providedIn: "root" })
export class AppearanceService {
  private readonly _tone = signal<string>(builtInTonePresets()[0].value);
  readonly tone = this._tone.asReadonly();

  setTone(value: string): void {
    this._tone.set(value);
  }

  restoreTone(value: string | undefined): void {
    if (value) this._tone.set(value);
  }
}