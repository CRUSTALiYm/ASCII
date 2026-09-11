import { Component, DestroyRef, effect, inject, signal } from "@angular/core";
import { ButtonModule } from "primeng/button";
import { TabsModule } from "primeng/tabs";

import { SourcePickerComponent } from "./source-picker/source-picker.component";
import { PreviewPanelComponent } from "./preview-panel/preview-panel.component";
import { BasicSettingsComponent } from "./basic-settings/basic-settings.component";
import { AdvancedSettingsComponent } from "./advanced-settings/advanced-settings.component";
import { HistoryPanelComponent } from "./history-panel/history-panel.component";
import { ProgressIndicatorComponent } from "./progress-indicator/progress-indicator.component";

import { SettingsService } from "../core/services/settings.service";
import { ConversionService } from "../core/services/conversion.service";
import { FileSourceService } from "../core/services/file-source.service";
import { CameraService } from "../core/services/camera.service";
import { ScreenService } from "../core/services/screen.service";
import { HistoryService } from "../core/services/history.service";
import { BackendsService } from "../core/services/backends.service";
import { AppearanceService } from "../core/services/appearance.service";
import { ExportService } from "../core/services/export.service";
import { NotificationsService } from "../core/services/notifications.service";
import { VideoSourceKind } from "../core/models/capture-source.model";

const AUTO_CONVERT_DEBOUNCE_MS = 150;

@Component({
  selector: "app-ascii-converter",
  imports: [
    ButtonModule,
    TabsModule,
    SourcePickerComponent,
    PreviewPanelComponent,
    BasicSettingsComponent,
    AdvancedSettingsComponent,
    HistoryPanelComponent,
    ProgressIndicatorComponent,
  ],
  templateUrl: "./ascii-converter.component.html",
  styleUrl: "./ascii-converter.component.scss",
})
export class AsciiConverterComponent {
  private readonly destroyRef = inject(DestroyRef);
  private readonly exportService = inject(ExportService);

  readonly settings = inject(SettingsService);
  readonly conversion = inject(ConversionService);
  readonly fileSource = inject(FileSourceService);
  readonly camera = inject(CameraService);
  readonly screen = inject(ScreenService);
  readonly history = inject(HistoryService);
  readonly backends = inject(BackendsService);
  readonly appearance = inject(AppearanceService);
  readonly notifications = inject(NotificationsService);

  readonly sourceKind = signal<VideoSourceKind>("file");

  private autoConvertTimer: ReturnType<typeof setTimeout> | null = null;

  constructor() {
    void this.history.init();
    void this.backends.ensureLoaded();
    void this.fileSource.listenForDrop((path) => void this.openDroppedFile(path));

    // Файл: любое изменение параметров пересчитывает превью тем же
    // вызовом (`conversion.service`), что и Export — с дебаунсом, чтобы не
    // слать запрос на каждое движение слайдера.
    effect(() => {
      const params = this.settings.params();
      const path = this.fileSource.path();
      if (this.sourceKind() !== "file" || !path) return;
      this.scheduleAutoConvert(path);
    });

    // Ошибки конвертации раньше просто оседали в conversion.error() и
    // никак не показывались — теперь всплывают тостом.
    effect(() => {
      const error = this.conversion.error();
      if (error) this.notifications.error(`Ошибка конвертации: ${error}`);
    });

    this.destroyRef.onDestroy(() => {
      this.camera.stop();
      this.screen.stop();
      this.fileSource.stopListeningForDrop();
      if (this.autoConvertTimer) clearTimeout(this.autoConvertTimer);
    });
  }

  async selectSource(kind: VideoSourceKind): Promise<void> {
    if (this.sourceKind() === kind) return;
    this.camera.stop();
    this.screen.stop();
    this.sourceKind.set(kind);

    if (kind === "camera") {
      await this.camera.loadDevices();
    } else if (kind === "screen") {
      await this.screen.loadSources();
    }
  }

  async pickAndOpenFile(): Promise<void> {
    const path = await this.fileSource.pickFile();
    if (path) await this.afterFileLoaded(path);
  }

  async openDroppedFile(path: string): Promise<void> {
    try {
      await this.fileSource.loadFile(path);
    } catch {
      return; // ошибка уже показана тостом внутри file-source.service
    }
    await this.afterFileLoaded(path);
  }

  startCameraStream(): void {
    this.camera.start(() => this.settings.params());
  }

  startScreenStream(): void {
    this.screen.start(() => this.settings.params());
  }

  saveToHistory(): void {
    const path = this.fileSource.path();
    if (!this.conversion.result() || !path) return;
    this.history.addEntry({
      name: path.split(/[\\/]/).pop() ?? path,
      path,
      settings: this.settings.params(),
      tone: this.appearance.tone(),
    });
  }

  async restoreFromHistory(id: string): Promise<void> {
    const entry = this.history.find(id);
    if (!entry) return;
    this.settings.patch(entry.settings);
    this.appearance.setTone(entry.tone);
    this.sourceKind.set("file");
    await this.fileSource.loadFile(entry.path);
    await this.conversion.convertFile(entry.path, this.settings.params());
  }

  exportText(): void {
    const result = this.conversion.result();
    if (result) void this.exportService.exportAsText(result);
  }

  exportImage(canvas: HTMLCanvasElement): void {
    void this.exportService.exportAsImage(canvas);
  }

  private async afterFileLoaded(path: string): Promise<void> {
    this.sourceKind.set("file");
    const best = await this.fileSource.findBestSettings(path);
    if (best) this.settings.update("columns", best.resolution);
    await this.conversion.convertFile(path, this.settings.params());
  }

  private scheduleAutoConvert(path: string): void {
    if (this.autoConvertTimer) clearTimeout(this.autoConvertTimer);
    this.autoConvertTimer = setTimeout(() => {
      void this.conversion.convertFile(path, this.settings.params());
    }, AUTO_CONVERT_DEBOUNCE_MS);
  }
}