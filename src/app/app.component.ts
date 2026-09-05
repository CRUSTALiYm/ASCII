import { Component } from "@angular/core";
import { RouterOutlet } from "@angular/router";
import { WindowTitlebarComponent } from "./window-titlebar/window-titlebar.component";

@Component({
  selector: "app-root",
  imports: [RouterOutlet, WindowTitlebarComponent],
  templateUrl: "./app.component.html",
  styleUrl: "./app.component.scss",
})
export class AppComponent {}
