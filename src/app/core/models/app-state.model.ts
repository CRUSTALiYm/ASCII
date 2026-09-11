import { AsciiParamsInput } from "./ascii-params.model";
import { HistoryItem } from "./history-item.model";

/** Версия формы `lastSettings`. Поднимайте при несовместимых изменениях
 * полей AsciiParams — settings.service сбрасывает то, что не совпадает,
 * вместо применения устаревших значений как есть. */
export const CURRENT_SETTINGS_VERSION = 4;

/** Форма, которую читает/пишет `load_app_state`/`save_app_state`
 * (src-tauri/src/lib/storage.rs). Rust хранит это как непрозрачный JSON —
 * форма целиком на совести фронтенда. */
export interface AppState {
  history: HistoryItem[];
  customCharacters: string[];
  customCharacterNames: Record<string, string>;
  settingsVersion: number;
  /** Последние использованные настройки — восстанавливаются при старте. */
  lastSettings?: Partial<AsciiParamsInput>;
  /** Appearance (цвет текста превью) — тоже восстанавливается при старте. */
  lastTone?: string;
  /** Сторона панели настроек ("left" | "right") — тоже appearance. */
  lastPanelPosition?: "left" | "right";
}

export function defaultAppState(): AppState {
  return {
    history: [],
    customCharacters: [],
    customCharacterNames: {},
    settingsVersion: CURRENT_SETTINGS_VERSION,
  };
}