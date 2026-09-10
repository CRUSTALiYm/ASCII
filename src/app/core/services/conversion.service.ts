import { Injectable, inject, signal } from "@angular/core";

import { TauriBridgeService } from "./tauri-bridge.service";
import { AsciiParamsInput } from "../models/ascii-params.model";
import {
  ConversionProgress,
  ConvertResult,
} from "../models/convert-result.model";

/** Единственный вызывающий код конвертации во всём приложении. Preview,
 * Export, камера и экран — все идут через `convertFile`/`convertFrame` с
 * одними и теми же параметрами, поэтому им физически негде разойтись
 * (тот самый фикс "Preview ≠ Output" на стороне клиента). */
@Injectable({ providedIn: "root" })
export class ConversionService {
  private readonly bridge = inject(TauriBridgeService);

  private readonly _result = signal<ConvertResult | null>(null);
  readonly result = this._result.asReadonly();

  private readonly _progress = signal<ConversionProgress | null>(null);
  readonly progress = this._progress.asReadonly();

  private readonly _isConverting = signal(false);
  readonly isConverting = this._isConverting.asReadonly();

  private readonly _error = signal<string | null>(null);
  readonly error = this._error.asReadonly();

  private currentJobId = 0;

  constructor() {
    void this.bridge.onConversionProgress((progress) => {
      if (progress.jobId === this.currentJobId) {
        this._progress.set(progress);
      }
    });
  }

  convertFile(
    path: string,
    params: AsciiParamsInput,
  ): Promise<ConvertResult | null> {
    return this.run((jobId) =>
      this.bridge.convertImageToAscii({ ...params, jobId, path }),
    );
  }

  convertFrame(
    frameBase64: string,
    params: AsciiParamsInput,
  ): Promise<ConvertResult | null> {
    return this.run((jobId) =>
      this.bridge.convertFrameToAscii({ ...params, jobId, frameBase64 }),
    );
  }

  async cancel(): Promise<void> {
    await this.bridge.cancelConversion();
    this._isConverting.set(false);
  }

  private async run(
    execute: (jobId: number) => Promise<ConvertResult>,
  ): Promise<ConvertResult | null> {
    const jobId = await this.bridge.beginConversion();
    this.currentJobId = jobId;
    this._isConverting.set(true);
    this._error.set(null);
    this._progress.set(null);

    try {
      const result = await execute(jobId);
      // Пока шёл запрос, могла начаться более новая конвертация — тогда
      // этот результат устарел и его применять не нужно.
      if (jobId !== this.currentJobId) return null;
      this._result.set(result);
      return result;
    } catch (error) {
      if (jobId === this.currentJobId) {
        this._error.set(String(error));
      }
      return null;
    } finally {
      if (jobId === this.currentJobId) {
        this._isConverting.set(false);
      }
    }
  }
}