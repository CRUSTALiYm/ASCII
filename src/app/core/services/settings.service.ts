import { Injectable, signal } from "@angular/core";
import {
  AsciiParamsInput,
  ComputeBackendId,
  defaultAsciiParams,
} from "../models/ascii-params.model";
import {
  CharacterPreset,
  builtInCharacterPresets,
} from "../models/character-preset.model";

function defaultParamsInput(): AsciiParamsInput {
  const { jobId: _jobId, ...rest } = defaultAsciiParams();
  return rest;
}

@Injectable({ providedIn: "root" })
export class SettingsService {
  private readonly _params = signal<AsciiParamsInput>(defaultParamsInput());
  readonly params = this._params.asReadonly();

  private readonly _presets = signal<CharacterPreset[]>(
    builtInCharacterPresets(),
  );
  readonly presets = this._presets.asReadonly();

  update<K extends keyof AsciiParamsInput>(
    key: K,
    value: AsciiParamsInput[K],
  ): void {
    this._params.update((current) => ({ ...current, [key]: value }));
  }

  patch(partial: Partial<AsciiParamsInput>): void {
    this._params.update((current) => ({ ...current, ...partial }));
  }

  reset(): void {
    this._params.set(defaultParamsInput());
  }

  setComputeBackend(id: ComputeBackendId): void {
    this.update("computeBackend", id);
  }

  // --- пресеты символов -----------------------------------------------------

  customPresets(): CharacterPreset[] {
    return this._presets().filter((preset) => preset.custom);
  }

  addCustomPreset(value: string, label?: string): CharacterPreset {
    const trimmed = value.trim();
    const preset: CharacterPreset = {
      label: label?.trim() || `Мой набор ${this.customPresets().length + 1}`,
      value: trimmed,
      description: `${trimmed.length} символов`,
      custom: true,
    };
    this._presets.update((list) => [...list, preset]);
    return preset;
  }

  updateCustomPreset(
    previousValue: string,
    value: string,
    label?: string,
  ): CharacterPreset | undefined {
    const trimmed = value.trim();
    let updated: CharacterPreset | undefined;
    this._presets.update((list) =>
      list.map((preset) => {
        if (preset.value !== previousValue || !preset.custom) return preset;
        updated = {
          ...preset,
          value: trimmed,
          label: label?.trim() || preset.label,
          description: `${trimmed.length} символов`,
        };
        return updated;
      }),
    );
    if (updated && this._params().characters === previousValue) {
      this.update("characters", updated.value);
    }
    return updated;
  }

  removeCustomPreset(value: string): void {
    this._presets.update((list) =>
      list.filter((preset) => preset.value !== value || !preset.custom),
    );
    if (this._params().characters === value) {
      this.update("characters", builtInCharacterPresets()[0].value);
    }
  }

  // --- восстановление из персистентного стейта (вызывает history.service) --

  restoreCustomCharacters(
    values: string[],
    names: Record<string, string>,
  ): void {
    if (!values.length) return;
    const restored: CharacterPreset[] = values.map((value, index) => ({
      label: names[value] || `Мой набор ${index + 1}`,
      value,
      description: `${value.length} символов`,
      custom: true,
    }));
    this._presets.update((list) => [...list, ...restored]);
  }

  restoreLastSettings(partial: Partial<AsciiParamsInput> | undefined): void {
    if (partial) this.patch(partial);
  }
}