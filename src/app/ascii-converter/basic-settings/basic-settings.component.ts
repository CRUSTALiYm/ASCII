import { Component, inject } from "@angular/core";
import { FormsModule } from "@angular/forms";
import { ButtonModule } from "primeng/button";
import { InputNumberModule } from "primeng/inputnumber";
import { SelectModule } from "primeng/select";
import { ToggleButtonModule } from "primeng/togglebutton";

import { SettingsService } from "../../core/services/settings.service";
import { CharacterPreset } from "../../core/models/character-preset.model";

@Component({
  selector: "app-basic-settings",
  imports: [
    FormsModule,
    InputNumberModule,
    SelectModule,
    ToggleButtonModule,
    ButtonModule,
  ],
  templateUrl: "./basic-settings.component.html",
  styleUrl: "./basic-settings.component.scss",
})
export class BasicSettingsComponent {
  readonly settings = inject(SettingsService);

  onColumnsChange(value: number | null): void {
    if (value === null) return;
    this.settings.update("columns", Math.max(8, Math.round(value)));
  }

  onCharactersChange(value: string): void {
    this.settings.update("characters", value);
  }

  onAdaptiveToggle(value: boolean): void {
    this.settings.update("adaptive", value);
  }

  selectedPreset(): CharacterPreset | undefined {
    const characters = this.settings.params().characters;
    return this.settings
      .presets()
      .find((preset) => preset.value === characters);
  }

  addCustomPreset(value: string): void {
    if (!value.trim()) return;
    const preset = this.settings.addCustomPreset(value);
    this.settings.update("characters", preset.value);
  }

  removeCustomPreset(preset: CharacterPreset): void {
    this.settings.removeCustomPreset(preset.value);
  }
}