import { Component } from "@angular/core";
import { WindowTitlebarComponent } from "./window-titlebar/window-titlebar.component";
import { AsciiConverterComponent } from "./ascii-converter/ascii-converter.component";

@Component({
  selector: "app-root",
  imports: [WindowTitlebarComponent, AsciiConverterComponent],
  templateUrl: "./app.component.html",
  styleUrl: "./app.component.scss",
})
export class AppComponent {}
