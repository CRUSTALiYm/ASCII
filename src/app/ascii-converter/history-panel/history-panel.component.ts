import { Component, EventEmitter, Output, inject } from "@angular/core";
import { DatePipe } from "@angular/common";
import { ButtonModule } from "primeng/button";

import { HistoryService } from "../../core/services/history.service";

@Component({
  selector: "app-history-panel",
  imports: [DatePipe, ButtonModule],
  templateUrl: "./history-panel.component.html",
  styleUrl: "./history-panel.component.scss",
})
export class HistoryPanelComponent {
  readonly history = inject(HistoryService);

  /** Восстановление требует fileSource+conversion — этим управляет
   * оболочка (ascii-converter.component.ts), поэтому здесь просто эмит. */
  @Output() readonly restore = new EventEmitter<string>();

  onRestore(id: string): void {
    this.restore.emit(id);
  }

  onRemove(id: string, event: Event): void {
    event.stopPropagation();
    this.history.removeEntry(id);
  }

  onClear(): void {
    void this.history.clear();
  }
}