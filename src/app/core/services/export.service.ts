import { Injectable, inject } from "@angular/core";
import { save } from "@tauri-apps/plugin-dialog";

import { TauriBridgeService } from "./tauri-bridge.service";
import { ConvertResult } from "../models/convert-result.model";
import {
  AsciiRenderOptions,
  createExportCanvas,
} from "../utils/ascii-canvas-render";

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

  /** Рендерит результат заново в отдельный canvas с разрешением,
   * привязанным к количеству символов (см. createExportCanvas), а не к
   * размеру панели превью на экране — иначе символы становятся
   * нечитаемыми при увеличении сохранённой картинки. `sourceAspect` берём
   * у preview-panel (`getSourceAspect()`), чтобы пропорции экспорта
   * совпадали с тем, что видел пользователь. */
  async exportAsImage(
    result: ConvertResult,
    sourceAspect: number,
    options: AsciiRenderOptions,
    suggestedName = "ascii-art.png",
  ): Promise<string | null> {
    const path = await save({
      defaultPath: suggestedName,
      filters: [{ name: "Изображение", extensions: ["png"] }],
    });
    if (!path) return null;

    const canvas = createExportCanvas(result, sourceAspect, options);
    const dataUrl = canvas.toDataURL("image/png");
    await this.bridge.saveExport(path, dataUrl, true);
    return path;
  }
}