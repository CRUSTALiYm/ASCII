import { Component, EventEmitter, Input, Output, inject } from "@angular/core";
import { FormsModule } from "@angular/forms";
import { ButtonModule } from "primeng/button";
import { SelectModule } from "primeng/select";

import { CameraService } from "../../core/services/camera.service";
import { ScreenService } from "../../core/services/screen.service";
import { VideoSourceKind } from "../../core/models/capture-source.model";

interface SourceOption {
  label: string;
  value: VideoSourceKind;
  icon: string;
}

@Component({
  selector: "app-source-picker",
  imports: [FormsModule, ButtonModule, SelectModule],
  templateUrl: "./source-picker.component.html",
  styleUrl: "./source-picker.component.scss",
})
export class SourcePickerComponent {
  readonly camera = inject(CameraService);
  readonly screen = inject(ScreenService);

  @Input() sourceKind: VideoSourceKind = "file";
  @Output() readonly sourceKindChange = new EventEmitter<VideoSourceKind>();
  @Output() readonly pickFile = new EventEmitter<void>();
  @Output() readonly startCamera = new EventEmitter<void>();
  @Output() readonly startScreen = new EventEmitter<void>();

  readonly sourceOptions: SourceOption[] = [
    { label: "Файл", value: "file", icon: "pi pi-image" },
    { label: "Камера", value: "camera", icon: "pi pi-video" },
    { label: "Экран", value: "screen", icon: "pi pi-desktop" },
  ];

  onSourceKindChange(kind: VideoSourceKind): void {
    this.sourceKindChange.emit(kind);
  }

  onCameraDeviceChange(index: number): void {
    this.camera.selectDevice(index);
    this.camera.stop();
  }

  onScreenSourceChange(id: string): void {
    this.screen.selectSource(id);
    this.screen.stop();
  }
}