import { Injectable, inject } from "@angular/core";
import { save } from "@tauri-apps/plugin-dialog";

import { TauriBridgeService } from "./tauri-bridge.service";
import { ConvertResult } from "../models/convert-result.model";

@Injectable({ providedIn: "root" })
export class ExportService {
  private readonly bridge = inject(TauriBridgeService);

  async exportAsText(
    result: ConvertResult,
    suggestedName = "ascii-art.txt",
  ): Promise<string | null> {
    const path = await save({
      defaultPath: suggestedName,
      filters: [{ name: "Текст", extensions: ["txt"] }],
    });
    if (!path) return null;
    await this.bridge.saveExport(path, result.text, false);
    return path;
  }

  /** `canvas` — тот же canvas, что рисует превью (ascii-canvas-render.ts),
   * поэтому экспортированная картинка пиксель в пиксель совпадает с тем,
   * что видел пользователь. */
  async exportAsImage(
    canvas: HTMLCanvasElement,
    suggestedName = "ascii-art.png",
  ): Promise<string | null> {
    const path = await save({
      defaultPath: suggestedName,
      filters: [{ name: "Изображение", extensions: ["png"] }],
    });
    if (!path) return null;
    const dataUrl = canvas.toDataURL("image/png");
    await this.bridge.saveExport(path, dataUrl, true);
    return path;
  }
}