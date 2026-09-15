import { Component, inject } from "@angular/core";
import { FormsModule } from "@angular/forms";
import { ButtonModule } from "primeng/button";
import { SelectModule } from "primeng/select";
import { ToggleButtonModule } from "primeng/togglebutton";

import { NumericScrubberComponent } from "../../shared/numeric-scrubber/numeric-scrubber.component";
import { SettingsService } from "../../core/services/settings.service";
import { CharacterPreset } from "../../core/models/character-preset.model";

@Component({
  selector: "app-basic-settings",
  imports: [
    FormsModule,
    NumericScrubberComponent,
    SelectModule,
    ToggleButtonModule,
    ButtonModule,
  ],
  templateUrl: "./basic-settings.component.html",
  styleUrl: "./basic-settings.component.scss",
})
export class BasicSettingsComponent {
  readonly settings = inject(SettingsService);

  // Клэмп на каждое изменение убран намеренно: если делать
  // Math.max(8, ...) при каждом нажатии клавиши, первая же введённая
  // цифра меньше 8 откатывает поле назад и не даёт набрать число целиком
  // (например "200"). Минимум и так гарантирован на бэкенде (columns.max(8)
  // в engine.rs) — фронту клэмпить незачем.
  onColumnsChange(value: number): void {
    this.settings.update("columns", Math.round(value));
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