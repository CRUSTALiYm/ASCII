import { Injectable, inject, signal } from "@angular/core";

import { TauriBridgeService } from "./tauri-bridge.service";
import {
  ComputeBackendInfo,
  RenderBackendInfo,
} from "../models/backend-info.model";
import { ComputeBackendId } from "../models/ascii-params.model";

@Injectable({ providedIn: "root" })
export class BackendsService {
  private readonly bridge = inject(TauriBridgeService);

  private readonly _computeBackends = signal<ComputeBackendInfo[]>([]);
  readonly computeBackends = this._computeBackends.asReadonly();

  private readonly _renderBackends = signal<RenderBackendInfo[]>([]);
  readonly renderBackends = this._renderBackends.asReadonly();

  private loaded = false;

  /** Грузит оба списка один раз за сессию; повторные вызовы — no-op. */
  async ensureLoaded(): Promise<void> {
    if (this.loaded) return;
    this.loaded = true;
    await Promise.all([this.loadComputeBackends(), this.loadRenderBackends()]);
  }

  async loadComputeBackends(): Promise<void> {
    this._computeBackends.set(await this.bridge.listComputeBackends());
  }

  async loadRenderBackends(): Promise<void> {
    this._renderBackends.set(await this.bridge.listRenderBackends());
  }

  /** "auto"/"cpu" всегда доступны — остальные проверяются по факту детекта. */
  isComputeBackendReady(id: ComputeBackendId): boolean {
    if (id === "auto" || id === "cpu") return true;
    return !!this._computeBackends().find((backend) => backend.id === id)
      ?.isAvailable;
  }

  /** Реально ли этот бэкенд посчитает на GPU, а не откатится на CPU внутри
   * движка (см. `executesOnGpu` в compute/mod.rs). */
  computeBackendExecutesOnGpu(id: ComputeBackendId): boolean {
    return !!this._computeBackends().find((backend) => backend.id === id)
      ?.executesOnGpu;
  }
}