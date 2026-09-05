import {
  AfterViewInit,
  Component,
  ElementRef,
  HostListener,
  OnDestroy,
  OnInit,
  ViewChild,
} from "@angular/core";
import { DatePipe } from "@angular/common";
import { FormsModule } from "@angular/forms";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import { ButtonModule } from "primeng/button";
import { DialogModule } from "primeng/dialog";
import { InputNumberModule } from "primeng/inputnumber";
import { SelectModule } from "primeng/select";
import { SliderModule } from "primeng/slider";
import { ToggleButtonModule } from "primeng/togglebutton";
import { ProgressBarModule } from "primeng/progressbar";

type ViewMode = "result" | "original" | "overlay" | "split";

interface HistoryItem {
  id: string;
  name: string;
  path: string;
  createdAt: string;
  settings: ConverterSettings;
}

interface PersistedState {
  history: HistoryItem[];
  customCharacters: string[];
  customCharacterNames?: Record<string, string>;
  settingsVersion?: number;
  settings?: Partial<ConverterSettings>;
}

interface ConverterSettings {
  selectedTone: string;
  selectedPreset: string;
  resolution: number;
  brightness: number;
  contrast: number;
  compression: number;
  invert: boolean;
  sourceBlackWhite: boolean;
  sourceInvert: boolean;
  sourceBrightness: number;
  sourceContrast: number;
  sourceThreshold: number;
}

type ExportFormat = "png" | "txt" | "all";

interface RustAsciiResult {
  columns: number;
  rows: number;
  values: string[];
  text: string;
}

interface ConversionProgress {
  jobId: number;
  processed: number;
  total: number;
}


interface CharacterPreset {
  label: string;
  value: string;
  description: string;
  custom?: boolean;
}

type ResettableSetting = keyof ConverterSettings;

@Component({
  selector: "app-ascii-converter",
  imports: [
    DatePipe,
    FormsModule,
    ButtonModule,
    DialogModule,
    InputNumberModule,
    SelectModule,
    SliderModule,
    ToggleButtonModule,
    ProgressBarModule,
  ],
  templateUrl: "./ascii-converter.component.html",
  styleUrl: "./ascii-converter.component.scss",
})
export class AsciiConverterComponent
  implements OnInit, AfterViewInit, OnDestroy
{
  @ViewChild("resultCanvas") resultCanvas?: ElementRef<HTMLCanvasElement>;
  @ViewChild("compareStage") compareStage?: ElementRef<HTMLElement>;
  @ViewChild("customPreviewCanvas") customPreviewCanvas?: ElementRef<HTMLCanvasElement>;
  @ViewChild("fileInput") fileInput?: ElementRef<HTMLInputElement>;
  @ViewChild("replaceFileInput") replaceFileInput?: ElementRef<HTMLInputElement>;

  readonly viewModes = [
    { label: "Результат", value: "result", icon: "pi pi-sparkles" },
    { label: "Оригинал", value: "original", icon: "pi pi-image" },
    { label: "Наложение", value: "overlay", icon: "pi pi-clone" },
    { label: "Слайдер", value: "split", icon: "pi pi-sliders-h" },
  ];
  readonly toneOptions = [
    { label: "Монохром", value: "mono" },
    { label: "Лайм", value: "lime" },
    { label: "Янтарь", value: "amber" },
    { label: "Лазурь", value: "cyan" },
  ];
  readonly presetOptions: CharacterPreset[] = [
    { label: "Classic", value: " .:-=+*#%@", description: "Мягкий контраст" },
    { label: "Editorial", value: " `.,-':<>;+!*/?%&98#@", description: "Точный газетный растр" },
    { label: "Blocks", value: " ░▒▓█", description: "Плотные блоки" },
    { label: "Matrix", value: " 01", description: "Цифровой ритм" },
    { label: "Minimal", value: " .#", description: "Минимум символов" },
  ];

  viewMode: ViewMode = "result";
  selectedTone = "mono";
  selectedPreset = this.presetOptions[0].value;
  resolution = 100;
  brightness = 0;
  contrast = 0;
  compression = 90;
  invert = false;
  sourceBlackWhite = false;
  sourceInvert = false;
  sourceBrightness = 0;
  sourceContrast = 0;
  sourceThreshold = 50;
  splitPosition = 50;
  imageUrl = "";
  imageName = "Нет изображения";
  imagePath = "";
  imageWidth = 0;
  imageHeight = 0;
  isFindingSettings = false;
  imageLoaded = false;
  isDragging = false;
  isLoading = true;
  showCharacterDialog = false;
  newCharacterSet = "";
  newCharacterName = "";
  editingCharacterValue = "";
  showExportDialog = false;
  exportFormat: ExportFormat = "png";
  exportBaseName = "ascii-result";
  history: HistoryItem[] = [];
  customCharacters: string[] = [];
  customCharacterNames: Record<string, string> = {};
  isSidebarCollapsed = false;
  isConverting = false;
  conversionProgress = 0;
  private rustResult?: RustAsciiResult;
  private asciiText = "";
  private sourceImage?: HTMLImageElement;
  private objectUrl?: string;
  private renderTimer?: ReturnType<typeof setTimeout>;
  private renderVersion = 0;
  private nativeJobId = 0;
  private progressUnlisten?: UnlistenFn;
  conversionMessage = "Обработка...";
  private settingsSearchVersion = 0;
  canvasDisplayWidth = 1;
  canvasDisplayHeight = 1;

  constructor() {}

  async ngOnInit(): Promise<void> {
    this.progressUnlisten = await listen<ConversionProgress>("conversion-progress", (event) => {
      if (event.payload.jobId === this.nativeJobId && event.payload.total > 0) {
        const pixelProgress = Math.round((event.payload.processed / event.payload.total) * 45);
        this.conversionProgress = Math.max(this.conversionProgress, pixelProgress);
      }
    });
    try {
      const state = await invoke<PersistedState>("load_app_state");
      this.history = state.history ?? [];
      this.customCharacters = state.customCharacters ?? [];
      this.customCharacterNames = state.customCharacterNames ?? {};
      if (state.settings) {
        Object.assign(this, state.settings);
        if (state.settingsVersion !== 3) {
          this.sourceBlackWhite = false;
          this.brightness = 0;
          this.contrast = 0;
          this.sourceBrightness = 0;
          this.sourceContrast = 0;
        }
      }
      this.presetOptions.push(
        ...this.customCharacters.map((value, index) => ({
          label: this.customCharacterNames[value] || `Мой набор ${index + 1}`,
          value,
          description: `${value.length} символов`,
          custom: true,
        })),
      );
    } catch {
      this.history = [];
    } finally {
      this.isLoading = false;
    }
  }

  ngAfterViewInit(): void {
    this.renderResult();
  }

  @HostListener("window:resize")
  onWindowResize(): void {
    this.fitCanvasToPreview();
  }

  ngOnDestroy(): void {
    this.progressUnlisten?.();
    if (this.objectUrl) {
      URL.revokeObjectURL(this.objectUrl);
    }
  }

  get hasImage(): boolean {
    return this.imageLoaded && Boolean(this.imageUrl);
  }

  get selectedPresetOption(): CharacterPreset {
    return (
      this.presetOptions.find((preset) => preset.value === this.selectedPreset) ??
      this.presetOptions[0]
    );
  }

  get displayedHistory(): HistoryItem[] {
    return this.history.slice(0, 6);
  }

  get hasHistory(): boolean {
    return this.history.length > 0;
  }

  get originalFilter(): string {
    const filters = [
      this.sourceBlackWhite ? "grayscale(1)" : "",
      this.sourceInvert || this.invert ? "invert(1)" : "",
      `brightness(${100 + this.sourceBrightness}%)`,
      `contrast(${100 + this.sourceContrast}%)`,
    ];
    return filters.filter(Boolean).join(" ");
  }

  get hasChanges(): boolean {
    return this.selectedTone !== "mono" || this.selectedPreset !== this.presetOptions[0].value || this.resolution !== 100 || this.brightness !== 0 || this.contrast !== 0 || this.compression !== 90 || this.invert || !this.sourceBlackWhite || this.sourceInvert || this.sourceBrightness !== 0 || this.sourceContrast !== 0 || this.sourceThreshold !== 50;
  }

  resetSettings(): void {
    this.selectedTone = "mono";
    this.selectedPreset = this.presetOptions[0].value;
    this.resolution = 100;
    this.brightness = 0;
    this.contrast = 0;
    this.compression = 90;
    this.invert = false;
    this.sourceBlackWhite = false;
    this.sourceInvert = false;
    this.sourceBrightness = 0;
    this.sourceContrast = 0;
    this.sourceThreshold = 50;
    this.updatePreview();
  }

  resetSetting(setting: ResettableSetting): void {
    const defaults: ConverterSettings = {
      selectedTone: "mono",
      selectedPreset: this.presetOptions[0].value,
      resolution: 100,
      brightness: 0,
      contrast: 0,
      compression: 90,
      invert: false,
      sourceBlackWhite: false,
      sourceInvert: false,
      sourceBrightness: 0,
      sourceContrast: 0,
      sourceThreshold: 50,
    };
    (this as unknown as Record<string, unknown>)[setting] = defaults[setting];
    this.updatePreview();
  }

  onFileSelected(event: Event): void {
    const input = event.target as HTMLInputElement;
    const file = input.files?.[0];
    if (file) {
      void this.loadFile(file);
    }
    input.value = "";
  }

  async openImageFile(): Promise<void> {
    try {
      const selected = await open({
        multiple: false,
        directory: false,
        filters: [{ name: "Изображения", extensions: ["png", "jpg", "jpeg", "webp", "gif", "bmp"] }],
      });
      if (typeof selected === "string") {
        await this.loadImagePath(selected, selected.split(/[\\/]/).pop() || "Изображение");
      }
    } catch {
      // Keep the browser/file-input path available when the Tauri dialog is unavailable.
      const input = this.hasImage ? this.replaceFileInput : this.fileInput;
      input?.nativeElement.click();
    }
  }

  private async loadImagePath(path: string, name: string): Promise<void> {
    this.cancelSettingsSearch();
    this.cancelNativeConversion();
    if (this.objectUrl) URL.revokeObjectURL(this.objectUrl);
    this.objectUrl = undefined;
    this.imagePath = path;
    this.imageName = name;
    this.imageUrl = convertFileSrc(path);
    this.imageLoaded = false;
    this.sourceImage = undefined;
    this.rustResult = undefined;
    this.asciiText = "";
    this.canvasDisplayWidth = 0;
    this.canvasDisplayHeight = 0;
    const image = new Image();
    image.onload = () => {
      this.sourceImage = image;
      this.imageLoaded = true;
      this.imageWidth = image.naturalWidth;
      this.imageHeight = image.naturalHeight;
      if (this.imagePath) void this.findBestInitialSettings();
    };
    image.onerror = () => {
      this.imageLoaded = false;
      this.conversionMessage = "Не удалось открыть изображение.";
    };
    image.src = this.imageUrl;
  }

  onDrop(event: DragEvent): void {
    event.preventDefault();
    this.isDragging = false;
    const file = event.dataTransfer?.files?.[0];
    if (file?.type.startsWith("image/")) {
      void this.loadFile(file);
    }
  }

  onDragOver(event: DragEvent): void {
    event.preventDefault();
    this.isDragging = true;
  }

  onDragLeave(): void {
    this.isDragging = false;
  }

  async loadFile(file: File): Promise<void> {
    if (!file.type.startsWith("image/")) return;
    const nativePath = (file as File & { path?: string }).path;
    if (nativePath) {
      await this.loadImagePath(nativePath, file.name);
      return;
    }
    this.cancelSettingsSearch();
    this.cancelNativeConversion();
    if (this.objectUrl) URL.revokeObjectURL(this.objectUrl);
    this.objectUrl = URL.createObjectURL(file);
    this.imageUrl = this.objectUrl;
    this.imageName = file.name;
    this.imagePath = "";
    this.imageLoaded = false;
    this.sourceImage = undefined;
    this.rustResult = undefined;
    this.asciiText = "";
    this.canvasDisplayWidth = 0;
    this.canvasDisplayHeight = 0;

    const image = new Image();
    image.onload = () => {
      this.sourceImage = image;
      this.imageLoaded = true;
      this.imageWidth = image.naturalWidth;
      this.imageHeight = image.naturalHeight;
      if (this.imagePath) void this.findBestInitialSettings();
    };
    image.src = this.imageUrl;
  }

  private async findBestInitialSettings(): Promise<void> {
    if (!this.sourceImage || !this.imagePath) return;
    const version = ++this.settingsSearchVersion;
    this.isFindingSettings = true;
    let settings: { resolution: number };
    try {
      const jobId = await invoke<number>("begin_settings_search");
      if (version !== this.settingsSearchVersion) {
        await invoke("cancel_settings_search");
        return;
      }
      settings = await invoke<{ resolution: number }>("find_best_settings", {
        path: this.imagePath,
        jobId,
      });
    } catch (error) {
      if (version === this.settingsSearchVersion && !String(error).includes("settings_search_cancelled")) {
        this.conversionMessage = "Не удалось подобрать параметры.";
      }
      return;
    } finally {
      if (version === this.settingsSearchVersion) {
        this.isFindingSettings = false;
      }
    }
    if (version !== this.settingsSearchVersion) return;
    this.resolution = settings.resolution;
    this.renderResult();
  }

  updatePreview(): void {
    this.cancelSettingsSearch();
    this.cancelNativeConversion();
    if (this.renderTimer) clearTimeout(this.renderTimer);
    this.renderTimer = setTimeout(() => this.renderResult(), 140);
  }

  previewWhileChanging(): void {
    this.resolutionChanged();
  }

  resolutionChanged(value?: number | null): void {
    if (value !== undefined) {
      this.resolution = Math.max(24, Math.min(300, Math.round(Number(value) || 24)));
    }
    this.cancelSettingsSearch();
    this.updatePreview();
  }

  private cancelNativeConversion(): void {
    this.renderVersion += 1;
    if (!this.nativeJobId) {
      void invoke("cancel_conversion");
      return;
    }
    this.nativeJobId = 0;
    void invoke("cancel_conversion");
  }

  private cancelSettingsSearch(): void {
    this.settingsSearchVersion += 1;
    this.isFindingSettings = false;
    void invoke("cancel_settings_search");
  }

  async saveToHistory(): Promise<void> {
    if (!this.imageLoaded) return;
    const item: HistoryItem = {
      id: `${Date.now()}`,
      name: this.imageName,
      path: this.imagePath,
      createdAt: new Date().toISOString(),
      settings: this.currentSettings(),
    };
    this.history = [item, ...this.history.filter((entry) => entry.path !== item.path)].slice(0, 20);
    await this.persistState();
  }

  async clearHistory(): Promise<void> {
    this.history = [];
    try {
      await invoke("clear_history");
    } catch {
      await this.persistState();
    }
  }

  async addCharacterSet(): Promise<void> {
    const value = this.newCharacterSet.trim();
    if (value.length < 2 || this.customCharacters.includes(value)) return;
    this.customCharacters = [...this.customCharacters, value];
    this.presetOptions.push({
      label: `Мой набор ${this.customCharacters.length}`,
      value,
      description: `${value.length} символов`,
      custom: true,
    });
    this.selectedPreset = value;
    this.newCharacterSet = "";
    this.showCharacterDialog = false;
    await this.persistState();
    this.renderResult();
  }

  openCharacterDialog(preset?: CharacterPreset): void {
    this.editingCharacterValue = preset?.custom ? preset.value : "";
    this.newCharacterSet = preset?.value ?? "";
    this.newCharacterName = preset?.custom ? preset.label : "";
    this.showCharacterDialog = true;
    setTimeout(() => this.renderCharacterPreview());
  }

  async saveCharacterSet(): Promise<void> {
    const value = this.newCharacterSet.trim();
    if (value.length < 2) return;
    if (this.editingCharacterValue) {
      const index = this.customCharacters.indexOf(this.editingCharacterValue);
      if (index < 0) return;
      this.customCharacters[index] = value;
      const name = this.newCharacterName.trim();
      if (name) this.customCharacterNames[value] = name;
      delete this.customCharacterNames[this.editingCharacterValue];
      const option = this.presetOptions.find((item) => item.value === this.editingCharacterValue);
      if (option) {
        option.value = value;
        option.label = name || `Мой набор ${index + 1}`;
        option.description = `${value.length} символов`;
      }
      if (this.selectedPreset === this.editingCharacterValue) this.selectedPreset = value;
    } else {
      if (this.customCharacters.includes(value)) return;
      this.customCharacters = [...this.customCharacters, value];
      if (this.newCharacterName.trim()) this.customCharacterNames[value] = this.newCharacterName.trim();
      this.presetOptions.push({ label: this.customCharacterNames[value] || `Мой набор ${this.customCharacters.length}`, value, description: `${value.length} символов`, custom: true });
      this.selectedPreset = value;
    }
    this.newCharacterSet = "";
    this.newCharacterName = "";
    this.editingCharacterValue = "";
    this.showCharacterDialog = false;
    await this.persistState();
    this.renderResult();
  }

  async deleteCharacterSet(preset: CharacterPreset): Promise<void> {
    if (!preset.custom) return;
    this.customCharacters = this.customCharacters.filter((value) => value !== preset.value);
    delete this.customCharacterNames[preset.value];
    const index = this.presetOptions.indexOf(preset);
    if (index >= 0) this.presetOptions.splice(index, 1);
    if (this.selectedPreset === preset.value) this.selectedPreset = this.presetOptions[0].value;
    await this.persistState();
    this.renderResult();
  }

  selectViewMode(mode: string): void {
    this.viewMode = mode as ViewMode;
    setTimeout(() => this.renderResult());
  }

  openExportDialog(): void {
    this.exportBaseName = this.imageName.replace(/\.[^.]+$/, "") || "ascii-result";
    this.showExportDialog = true;
  }

  async exportResult(): Promise<void> {
    const canvas = this.resultCanvas?.nativeElement;
    if (!canvas) return;
    const extension = this.exportFormat === "txt" ? "txt" : "png";
    const selectedPath = await save({
      defaultPath: `${this.exportBaseName}.${extension}`,
      filters: [{ name: this.exportFormat === "txt" ? "ASCII text" : "PNG image", extensions: [extension] }],
    });
    if (!selectedPath) return;
    const text = this.rustResult?.text ?? this.asciiText;
    const png = canvas.toDataURL("image/png", 1);
    if (this.exportFormat === "all") {
      const basePath = selectedPath.replace(/\.[^.\\/]+$/, "");
      await invoke("save_export", { path: `${basePath}.png`, content: png, binary: true });
      await invoke("save_export", { path: `${basePath}.txt`, content: text, binary: false });
    } else {
      await invoke("save_export", {
        path: selectedPath,
        content: this.exportFormat === "txt" ? text : png,
        binary: this.exportFormat === "png",
      });
    }
    await this.saveToHistory();
    this.showExportDialog = false;
  }

  renderResult(): void {
    const version = ++this.renderVersion;
    this.isConverting = true;
    this.conversionProgress = 0;
    this.conversionMessage = "Обработка...";
    if (!this.sourceImage || !this.resultCanvas) {
      setTimeout(() => this.sourceImage && this.renderResult());
      return;
    }
    if (!this.imagePath || this.imagePath.startsWith("blob:") || this.imagePath === this.imageName) {
      this.isConverting = false;
      this.conversionMessage = "Не удалось получить путь к файлу.";
      return;
    }
    void this.renderRustResult(version);
  }

  private async renderRustResult(version: number): Promise<void> {
    if (!this.imagePath || !this.resultCanvas) return;
    this.isConverting = true;
    let activeJobId = 0;
    try {
      const jobId = await invoke<number>("begin_conversion");
      if (version !== this.renderVersion) {
        await invoke("cancel_conversion");
        return;
      }
      this.nativeJobId = jobId;
      activeJobId = jobId;
      this.rustResult = await invoke<RustAsciiResult>("convert_image_to_ascii", {
        request: {
          jobId,
          path: this.imagePath,
          columns: this.resolution,
          characters: this.selectedPreset,
          brightness: this.brightness,
          contrast: this.contrast,
          invert: this.invert,
          sourceBrightness: this.sourceBrightness,
          sourceContrast: this.sourceContrast,
          sourceInvert: this.sourceInvert,
          sourceBlackWhite: this.sourceBlackWhite,
          sourceThreshold: this.sourceThreshold,
        },
      });
      this.conversionProgress = Math.max(this.conversionProgress, 50);
      if (version !== this.renderVersion) return;
      const metrics = this.measureCharacter();
      const canvas = this.resultCanvas.nativeElement;
      canvas.width = this.rustResult.columns * metrics.width;
      canvas.height = this.rustResult.rows * metrics.height;
      this.fitCanvasToPreview();
      const context = canvas.getContext("2d");
      if (!context) return;
      context.fillStyle = "#101217";
      context.fillRect(0, 0, canvas.width, canvas.height);
      context.font = `${metrics.fontSize}px monospace`;
      context.textBaseline = "top";
      context.fillStyle = this.toneColor();
      const resultRows = this.rustResult.rows;
      const resultColumns = this.rustResult.columns;
      for (let y = 0; y < resultRows; y += 1) {
        if (version !== this.renderVersion) return;
        const start = y * resultColumns;
        const line = this.rustResult.values.slice(start, start + resultColumns).join("");
        context.fillText(line, 0, y * metrics.height);
        if (y % 4 === 0 || y === resultRows - 1) {
          this.conversionProgress = Math.max(
            this.conversionProgress,
            50 + Math.round(((y + 1) / resultRows) * 50),
          );
          await new Promise<void>((resolve) => setTimeout(resolve, 0));
        }
      }
      this.asciiText = this.rustResult.text;
      this.renderCustomPreview();
      this.conversionProgress = 100;
    } catch (error) {
      const reason = String(error);
      if (version === this.renderVersion && !reason.includes("conversion_cancelled")) {
        this.conversionMessage = "Не удалось обработать изображение в Rust.";
      }
    } finally {
      if (this.nativeJobId === activeJobId) this.nativeJobId = 0;
      if (version === this.renderVersion) this.isConverting = false;
    }
  }

  private measureCharacter(): { width: number; height: number; fontSize: number } {
    const probe = document.createElement("canvas").getContext("2d");
    if (!probe) return { width: 9, height: 16, fontSize: 14 };
    const fontSize = 14;
    probe.font = `${fontSize}px monospace`;
    const metrics = probe.measureText("M");
    return {
      width: Math.ceil(metrics.width),
      height: Math.ceil((metrics.actualBoundingBoxAscent || 11) + (metrics.actualBoundingBoxDescent || 3) + 2),
      fontSize,
    };
  }

  private fitCanvasToPreview(): void {
    requestAnimationFrame(() => requestAnimationFrame(() => {
      const canvas = this.resultCanvas?.nativeElement;
      const stage = this.compareStage?.nativeElement;
      if (!canvas || !stage || !canvas.width || !canvas.height) return;
      const availableWidth = Math.max(1, stage.clientWidth - 20);
      const availableHeight = Math.max(1, stage.clientHeight - 20);
      const fit = Math.min(availableWidth / canvas.width, availableHeight / canvas.height);
      this.canvasDisplayWidth = Math.max(1, Math.round(canvas.width * fit));
      this.canvasDisplayHeight = Math.max(1, Math.round(canvas.height * fit));
    }));
  }

  private renderCustomPreview(): void {
    const preview = this.customPreviewCanvas?.nativeElement;
    const result = this.resultCanvas?.nativeElement;
    if (!preview || !result) return;
    preview.width = 360;
    preview.height = 180;
    const context = preview.getContext("2d");
    if (!context) return;
    context.fillStyle = "#101217";
    context.fillRect(0, 0, preview.width, preview.height);
    context.drawImage(result, 0, 0, preview.width, preview.height);
  }

  renderCharacterPreview(): void {
    this.renderCustomPreview();
  }

  private toneColor(): string {
    return {
      mono: "#e6e8ee",
      lime: "#b7f36b",
      amber: "#ffc857",
      cyan: "#6de4ff",
    }[this.selectedTone] ?? "#e6e8ee";
  }

  private async persistState(): Promise<void> {
    try {
      await invoke("save_app_state", {
        state: {
          history: this.history,
          customCharacters: this.customCharacters,
          customCharacterNames: this.customCharacterNames,
          settingsVersion: 3,
          settings: {
            ...this.currentSettings(),
          },
        } satisfies PersistedState,
      });
    } catch {
      // The browser preview can run without the Tauri runtime.
    }
  }

  private currentSettings(): ConverterSettings {
    return {
      selectedTone: this.selectedTone,
      selectedPreset: this.selectedPreset,
      resolution: this.resolution,
      brightness: this.brightness,
      contrast: this.contrast,
      compression: this.compression,
      invert: this.invert,
      sourceBlackWhite: this.sourceBlackWhite,
      sourceInvert: this.sourceInvert,
      sourceBrightness: this.sourceBrightness,
      sourceContrast: this.sourceContrast,
      sourceThreshold: this.sourceThreshold,
    };
  }
}