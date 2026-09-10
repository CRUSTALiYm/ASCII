import { Injectable, inject, signal } from "@angular/core";

import { TauriBridgeService } from "./tauri-bridge.service";
import { ConversionService } from "./conversion.service";
import { CameraDeviceInfo } from "../models/capture-source.model";
import { AsciiParamsInput } from "../models/ascii-params.model";

@Injectable({ providedIn: "root" })
export class CameraService {
  private readonly bridge = inject(TauriBridgeService);
  private readonly conversion = inject(ConversionService);

  private readonly _devices = signal<CameraDeviceInfo[]>([]);
  readonly devices = this._devices.asReadonly();

  private readonly _selectedIndex = signal<number | null>(null);
  readonly selectedIndex = this._selectedIndex.asReadonly();

  private readonly _isStreaming = signal(false);
  readonly isStreaming = this._isStreaming.asReadonly();

  private readonly _lastFrameBase64 = signal<string | null>(null);
  readonly lastFrameBase64 = this._lastFrameBase64.asReadonly();

  private loopHandle: ReturnType<typeof setTimeout> | null = null;
  private streamToken = 0;

  async loadDevices(): Promise<void> {
    this._devices.set(await this.bridge.listCameraDevices());
  }

  selectDevice(index: number): void {
    this._selectedIndex.set(index);
  }

  /** Запускает цикл живого превью: захват кадра → конвертация → пауза →
   * повтор. `getParams` вызывается на каждом тике, а не один раз при
   * старте — так изменения слайдеров применяются на следующем же кадре. */
  start(getParams: () => AsciiParamsInput, intervalMs = 200): void {
    const index = this._selectedIndex();
    if (index === null) return;

    this.stop();
    this._isStreaming.set(true);
    const token = ++this.streamToken;

    const tick = async (): Promise<void> => {
      if (token !== this.streamToken) return;
      try {
        const frame = await this.bridge.captureCameraFrame(index);
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