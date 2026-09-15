import { Injectable, effect, inject, signal } from "@angular/core";

import { TauriBridgeService } from "./tauri-bridge.service";
import { SettingsService } from "./settings.service";
import { AppearanceService } from "./appearance.service";
import {
  AppState,
  CURRENT_SETTINGS_VERSION,
  defaultAppState,
} from "../models/app-state.model";
import { HistoryItem } from "../models/history-item.model";

const SAVE_DEBOUNCE_MS = 800;

/** Единственное место, которое читает/пишет `AppState` целиком. Дебаунсит
 * автосохранение при изменении настроек/пресетов и сразу сохраняет при
 * изменениях самой истории. */
@Injectable({ providedIn: "root" })
export class HistoryService {
  private readonly bridge = inject(TauriBridgeService);
  private readonly settings = inject(SettingsService);
  private readonly appearance = inject(AppearanceService);

  private readonly _history = signal<HistoryItem[]>([]);
  readonly history = this._history.asReadonly();

  private readonly _ready = signal(false);
  readonly ready = this._ready.asReadonly();

  private saveTimer: ReturnType<typeof setTimeout> | null = null;

  constructor() {
    effect(() => {
      this.settings.params();
      this.settings.presets();
      this.appearance.tone();
      this.appearance.background();
      this.appearance.panelPosition();
      if (this._ready()) this.scheduleSave();
    });
  }

  async init(): Promise<void> {
    let state: AppState;
    try {
      state = await this.bridge.loadAppState();
    } catch {
      state = defaultAppState();
    }

    if (state.settingsVersion !== CURRENT_SETTINGS_VERSION) {
      // Несовместимая версия — берём только историю и наборы символов,
      // сами настройки остаются дефолтными (уже стоят в settings.service).
      state = {
        ...defaultAppState(),
        history: state.history ?? [],
        customCharacters: state.customCharacters ?? [],
        customCharacterNames: state.customCharacterNames ?? {},
      };
    }

    this._history.set(state.history ?? []);
    this.settings.restoreCustomCharacters(
      state.customCharacters ?? [],
      state.customCharacterNames ?? {},
    );
    this.settings.restoreLastSettings(state.lastSettings);
    this.appearance.restoreTone(state.lastTone);
    this.appearance.restoreBackground(state.lastBackground);
    this.appearance.restorePanelPosition(state.lastPanelPosition);
    this._ready.set(true);
  }

  addEntry(item: Omit<HistoryItem, "id" | "createdAt">): HistoryItem {
    const entry: HistoryItem = {
      ...item,
      id: crypto.randomUUID(),
      createdAt: new Date().toISOString(),
    };
    this._history.update((list) => [entry, ...list]);
    this.scheduleSave(true);
    return entry;
  }

  removeEntry(id: string): void {
    this._history.update((list) => list.filter((entry) => entry.id !== id));
    this.scheduleSave(true);
  }

  async clear(): Promise<void> {
    this._history.set([]);
    await this.bridge.clearHistory();
  }

  find(id: string): HistoryItem | undefined {
    return this._history().find((entry) => entry.id === id);
  }

  private scheduleSave(immediate = false): void {
    if (this.saveTimer) clearTimeout(this.saveTimer);
    if (immediate) {
      void this.persist();
      return;
    }
    this.saveTimer = setTimeout(() => void this.persist(), SAVE_DEBOUNCE_MS);
  }

  private async persist(): Promise<void> {
    const customPresets = this.settings.customPresets();
    const state: AppState = {
      history: this._history(),
      customCharacters: customPresets.map((preset) => preset.value),
      customCharacterNames: Object.fromEntries(
        customPresets.map((preset) => [preset.value, preset.label]),
      ),
      settingsVersion: CURRENT_SETTINGS_VERSION,
      lastSettings: this.settings.params(),
      lastTone: this.appearance.tone(),
      lastBackground: this.appearance.background(),
      lastPanelPosition: this.appearance.panelPosition(),
    };
    await this.bridge.saveAppState(state);
  }
}