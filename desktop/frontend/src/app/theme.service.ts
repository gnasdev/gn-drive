import { Injectable } from '@angular/core';
import { BehaviorSubject } from 'rxjs';

@Injectable({
  providedIn: 'root'
})
export class ThemeService {
  private isDarkModeSubject = new BehaviorSubject<boolean>(false);
  public isDarkMode$ = this.isDarkModeSubject.asObservable();

  constructor() {
    this.initializeTheme();
  }

  private initializeTheme(): void {
    // Check localStorage first
    const savedTheme = localStorage.getItem('dark-mode');
    let isDarkMode: boolean;

    if (savedTheme !== null) {
      isDarkMode = savedTheme === 'true';
    } else {
      // Check system preference
      isDarkMode = window.matchMedia('(prefers-color-scheme: dark)').matches;
    }

    this.setDarkMode(isDarkMode);
  }

  public toggleDarkMode(): void {
    const newMode = !this.isDarkModeSubject.value;
    this.setDarkMode(newMode);
  }

  public setDarkMode(isDarkMode: boolean): void {
    console.log('ThemeService: Setting dark mode to', isDarkMode);
    
    // Update the subject
    this.isDarkModeSubject.next(isDarkMode);
    
    // Save to localStorage
    localStorage.setItem('dark-mode', isDarkMode.toString());
    
    // Apply theme to DOM
    this.applyTheme(isDarkMode);
  }

  public get isDarkMode(): boolean {
    return this.isDarkModeSubject.value;
  }

  private applyTheme(isDarkMode: boolean): void {
    const body = document.body;
    const html = document.documentElement;
    
    // Remove existing theme classes
    body.classList.remove('dark-theme', 'light-theme');
    html.classList.remove('dark-theme', 'light-theme');
    
    if (isDarkMode) {
      body.classList.add('dark-theme');
      html.classList.add('dark-theme');
      console.log('ThemeService: Applied dark theme');
    } else {
      body.classList.add('light-theme');
      html.classList.add('light-theme');
      console.log('ThemeService: Applied light theme');
    }

    // Force repaint to ensure styles are applied
    setTimeout(() => {
      body.style.display = 'none';
      body.offsetHeight; // trigger reflow
      body.style.display = '';
    }, 0);
  }
}
