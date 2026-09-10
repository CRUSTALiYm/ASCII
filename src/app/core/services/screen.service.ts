import { Injectable, inject, signal } from "@angular/core";

import { TauriBridgeService } from "./tauri-bridge.service";
import { ConversionService } from "./conversion.service";
import { ScreenSourceInfo } from "../models/capture-source.model";
import { AsciiParamsInput } from "../models/ascii-params.model";

@Injectable({ providedIn: "root" })
export class ScreenService {
  private readonly bridge = inject(TauriBridgeService);
  private readonly conversion = inject(ConversionService);

  private readonly _sources = signal<ScreenSourceInfo[]>([]);
  readonly sources = this._sources.asReadonly();

  private readonly _selectedId = signal<string | null>(null);
  readonly selectedId = this._selectedId.asReadonly();

  private readonly _isStreaming = signal(false);
  readonly isStreaming = this._isStreaming.asReadonly();

  private readonly _lastFrameBase64 = signal<string | null>(null);
  readonly lastFrameBase64 = this._lastFrameBase64.asReadonly();

  private loopHandle: ReturnType<typeof setTimeout> | null = null;
  private streamToken = 0;

  async loadSources(): Promise<void> {
    this._sources.set(await this.bridge.listScreenSources());
  }

  selectSource(id: string): void {
    this._selectedId.set(id);
  }

  /** Тот же цикл, что и в camera.service.ts: захват → конвертация → пауза →
   * повтор, с `getParams` на каждом тике. */
  start(getParams: () => AsciiParamsInput, intervalMs = 200): void {
    const id = this._selectedId();
    if (id === null) return;

    this.stop();
    this._isStreaming.set(true);
    const token = ++this.streamToken;

    const tick = async (): Promise<void> => {
      if (token !== this.streamToken) return;
      try {
        const frame = await this.bridge.captureScreenFrame(id);
        if (token !== this.streamToken) return;
        this._lastFrameBase64.set(frame);
        await this.conversion.convertFrame(frame, getParams());
      } catch {
        // Кадр не удалось захватить — пробуем на следующем тике, поток не роняем.
      }
      if (token === this.streamToken) {
        this.loopHandle = setTimeout(() => void tick(), intervalMs);
      }
    };

    void tick();
  }

  stop(): void {
    this.streamToken++;
    this._isStreaming.set(false);
    if (this.loopHandle) {
      clearTimeout(this.loopHandle);
      this.loopHandle = null;
    }
  }
}