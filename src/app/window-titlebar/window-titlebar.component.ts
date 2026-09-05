import { Component } from "@angular/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

const appWindow = getCurrentWindow();

@Component({
  selector: "app-window-titlebar",
  templateUrl: "./window-titlebar.component.html",
  styleUrl: "./window-titlebar.component.scss",
})
export class WindowTitlebarComponent {
  startDragging(event: MouseEvent): void {
    if (event.button === 0 && event.detail === 1) {
      event.preventDefault();
      event.stopPropagation();
      void appWindow.startDragging();
    }
  }

  minimizeWindow(): void {
    void appWindow.minimize();
  }

  toggleMaximizeWindow(event?: MouseEvent): void {
    event?.preventDefault();
    event?.stopPropagation();
    void appWindow.toggleMaximize();
  }

  closeWindow(): void {
    void appWindow.close();
  }
}