import { Component, inject } from "@angular/core";
import { FormsModule } from "@angular/forms";
import { InputNumberModule } from "primeng/inputnumber";
import { SelectModule } from "primeng/select";
import { SliderModule } from "primeng/slider";
import { TagModule } from "primeng/tag";
import { ToggleButtonModule } from "primeng/togglebutton";

import { SettingsService } from "../../core/services/settings.service";
import { BackendsService } from "../../core/services/backends.service";
import {
  AsciiParamsInput,
  ComputeBackendId,
} from "../../core/models/ascii-params.model";

type NumericParamKey = Extract<
  keyof AsciiParamsInput,
  | "brightness"
  | "contrast"
  | "sourceBrightness"
  | "sourceContrast"
  | "sourceThreshold"
  | "adaptiveRegionTiles"
  | "adaptivePercentile"
  | "adaptiveSmoothing"
  | "adaptiveMinRange"
  | "adaptiveDespikeThreshold"
>;

type BooleanParamKey = Extract<
  keyof AsciiParamsInput,
  "invert" | "sourceInvert" | "sourceBlackWhite"
>;

interface ComputeBackendOption {
  label: string;
  value: ComputeBackendId;
}

@Component({
  selector: "app-advanced-settings",
  imports: [
    FormsModule,
    InputNumberModule,
    SelectModule,
    SliderModule,
    TagModule,
    ToggleButtonModule,
  ],
  templateUrl: "./advanced-settings.component.html",
  styleUrl: "./advanced-settings.component.scss",
})
export class AdvancedSettingsComponent {
  readonly settings = inject(SettingsService);
  readonly backends = inject(BackendsService);

  readonly computeBackendOptions: ComputeBackendOption[] = [
    { label: "Авто", value: "auto" },
    { label: "CPU", value: "cpu" },
    { label: "CUDA", value: "cuda" },
    { label: "WebGPU", value: "webgpu" },
    { label: "OpenCL", value: "opencl" },
    { label: "DirectCompute", value: "directcompute" },
  ];

  setNumber(key: NumericParamKey, value: number | null): void {
    if (value === null) return;
    this.settings.update(key, value);
  }

  setBoolean(key: BooleanParamKey, value: boolean): void {
    this.settings.update(key, value);
  }

  onComputeBackendChange(id: ComputeBackendId): void {
    this.settings.setComputeBackend(id);
  }

  isBackendReady(id: ComputeBackendId): boolean {
    return this.backends.isComputeBackendReady(id);
  }

  backendExecutesOnGpu(id: ComputeBackendId): boolean {
    return this.backends.computeBackendExecutesOnGpu(id);
  }
}