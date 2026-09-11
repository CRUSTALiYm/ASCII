import { Component, computed, inject } from "@angular/core";
import { ProgressBarModule } from "primeng/progressbar";

import { ConversionService } from "../../core/services/conversion.service";

@Component({
  selector: "app-progress-indicator",
  imports: [ProgressBarModule],
  templateUrl: "./progress-indicator.component.html",
  styleUrl: "./progress-indicator.component.scss",
})
export class ProgressIndicatorComponent {
  readonly conversion = inject(ConversionService);

  readonly percent = computed(() => {
    const progress = this.conversion.progress();
    if (!progress || progress.total <= 0) return 0;
    return Math.round((progress.processed / progress.total) * 100);
  });
}