import { Injectable } from "@angular/core";
import { BehaviorSubject } from "rxjs";

@Injectable({
  providedIn: "root",
})
export class ThemeService {
  private isDarkModeSubject = new BehaviorSubject<boolean>(true);
  public isDarkMode$ = this.isDarkModeSubject.asObservable();

  constructor() {
    this.initializeTheme();
  }

  private initializeTheme(): void {
    // Always use dark mode
    this.applyTheme();
  }

  public get isDarkMode(): boolean {
    return true; // Always dark mode
  }

  private applyTheme(): void {
    const body = document.body;
    const html = document.documentElement;

    // Remove any existing theme classes and apply dark theme
    body.classList.remove("light-theme");
    html.classList.remove("light-theme");
    body.classList.add("dark-theme");
    html.classList.add("dark-theme");

    console.log("ThemeService: Applied dark theme (permanent)");

    // Force repaint to ensure styles are applied
    setTimeout(() => {
      body.style.display = "none";
      body.offsetHeight; // trigger reflow
      body.style.display = "";
    }, 0);
  }
}
