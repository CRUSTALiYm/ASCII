import { Injectable, inject, signal } from "@angular/core";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { UnlistenFn } from "@tauri-apps/api/event";

import { TauriBridgeService } from "./tauri-bridge.service";
import { NotificationsService } from "./notifications.service";
import { BestSettings } from "../models/convert-result.model";

const IMAGE_EXTENSIONS = ["png", "jpg", "jpeg", "webp", "bmp", "gif"];
const MIME_BY_EXTENSION: Record<string, string> = {
  png: "image/png",
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  webp: "image/webp",
  bmp: "image/bmp",
  gif: "image/gif",
};

function extensionOf(path: string): string {
  const dot = path.lastIndexOf(".");
  return dot === -1 ? "" : path.slice(dot + 1).toLowerCase();
}

function mimeFor(path: string): string {
  return MIME_BY_EXTENSION[extensionOf(path)] ?? "image/png";
}

@Injectable({ providedIn: "root" })
export class FileSourceService {
  private readonly bridge = inject(TauriBridgeService);
  private readonly notifications = inject(NotificationsService);

  private readonly _path = signal<string | null>(null);
  readonly path = this._path.asReadonly();

  /** base64 data URL исходника — нужен для рендера режима "Оригинал" тем
   * же движком, что и результат (см. пояснение к conversion.service.ts). */
  private readonly _originalDataUrl = signal<string | null>(null);
  readonly originalDataUrl = this._originalDataUrl.asReadonly();

  private settingsJobId = 0;
  private dragUnlisten: UnlistenFn | null = null;

  /** Раньше сбой здесь выглядел как "нажал — ничего не произошло": любая
   * ошибка (диалог, чтение файла) тонула в try/catch. Теперь она видима
   * через notifications.service и попадает в консоль для отладки. */
  async pickFile(): Promise<string | null> {
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: "Изображения", extensions: IMAGE_EXTENSIONS }],
      });
      if (!selected || Array.isArray(selected)) return null;
      await this.loadFile(selected);
      return selected;
    } catch (error) {
      console.error("[file-source] pickFile failed", error);
      this.notifications.error(`Не удалось открыть файл: ${String(error)}`);
      return null;
    }
  }

  async loadFile(path: string): Promise<void> {
    try {
      const base64 = await this.bridge.readImageAsBase64(path);
      this._path.set(path);
      this._originalDataUrl.set(`data:${mimeFor(path)};base64,${base64}`);
    } catch (error) {
      console.error("[file-source] loadFile failed", error);
      this.notifications.error(`Не удалось прочитать файл: ${String(error)}`);
      throw error;
    }
  }

  async findBestSettings(path: string): Promise<BestSettings | null> {
    const jobId = await this.bridge.beginSettingsSearch();
    this.settingsJobId = jobId;
    try {
      const result = await this.bridge.findBestSettings(path, jobId);
      return jobId === this.settingsJobId ? result : null;
    } catch (error) {
      console.error("[file-source] findBestSettings failed", error);
      return null;
    }
  }

  cancelBestSettingsSearch(): Promise<void> {
    return this.bridge.cancelSettingsSearch();
  }

  /** Реальные пути при drag&drop файла в окно — браузерный File API таких
   * путей не даёт, нужен window-level Tauri-эвент. */
  async listenForDrop(onDrop: (path: string) => void): Promise<void> {
    this.dragUnlisten?.();
    this.dragUnlisten = await getCurrentWindow().onDragDropEvent((event) => {
      if (event.payload.type !== "drop") return;
      const [firstPath] = event.payload.paths;
      if (firstPath && IMAGE_EXTENSIONS.includes(extensionOf(firstPath))) {
        onDrop(firstPath);
      }
    });
  }

  stopListeningForDrop(): void {
    this.dragUnlisten?.();
    this.dragUnlisten = null;
  }
}