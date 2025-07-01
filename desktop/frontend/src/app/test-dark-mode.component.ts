import { Component } from '@angular/core';
import { CommonModule } from '@angular/common';
import { MatButtonModule } from '@angular/material/button';
import { MatCardModule } from '@angular/material/card';
import { MatIconModule } from '@angular/material/icon';
import { ThemeService } from './theme.service';

@Component({
  selector: 'app-test-dark-mode',
  standalone: true,
  imports: [CommonModule, MatButtonModule, MatCardModule, MatIconModule],
  template: `
    <mat-card class="test-card">
      <mat-card-header>
        <mat-icon mat-card-avatar>palette</mat-icon>
        <mat-card-title>Dark Mode Test</mat-card-title>
        <mat-card-subtitle>Test the dark mode toggle functionality</mat-card-subtitle>
      </mat-card-header>
      
      <mat-card-content>
        <p>Current theme: {{ themeService.isDarkMode ? 'Dark' : 'Light' }}</p>
        <p>Body classes: {{ getBodyClasses() }}</p>
        <p>HTML classes: {{ getHtmlClasses() }}</p>
        <p>LocalStorage value: {{ getLocalStorageValue() }}</p>
      </mat-card-content>
      
      <mat-card-actions>
        <button mat-raised-button color="primary" (click)="toggleTheme()">
          <mat-icon>{{ themeService.isDarkMode ? 'light_mode' : 'dark_mode' }}</mat-icon>
          Toggle to {{ themeService.isDarkMode ? 'Light' : 'Dark' }} Mode
        </button>
        
        <button mat-button (click)="logThemeInfo()">
          <mat-icon>info</mat-icon>
          Log Theme Info
        </button>
      </mat-card-actions>
    </mat-card>
  `,
  styles: [`
    .test-card {
      margin: 16px;
      max-width: 500px;
    }
    
    mat-card-actions {
      display: flex;
      gap: 8px;
      flex-wrap: wrap;
    }
    
    mat-card-content p {
      margin: 8px 0;
      font-family: monospace;
      font-size: 14px;
    }
  `]
})
export class TestDarkModeComponent {
  constructor(public themeService: ThemeService) {}

  toggleTheme(): void {
    console.log('Before toggle:', this.themeService.isDarkMode);
    this.themeService.toggleDarkMode();
    console.log('After toggle:', this.themeService.isDarkMode);
  }

  getBodyClasses(): string {
    return Array.from(document.body.classList).join(', ') || 'none';
  }

  getHtmlClasses(): string {
    return Array.from(document.documentElement.classList).join(', ') || 'none';
  }

  getLocalStorageValue(): string {
    return localStorage.getItem('dark-mode') || 'null';
  }

  logThemeInfo(): void {
    console.log('=== Theme Info ===');
    console.log('ThemeService.isDarkMode:', this.themeService.isDarkMode);
    console.log('Body classes:', this.getBodyClasses());
    console.log('HTML classes:', this.getHtmlClasses());
    console.log('LocalStorage:', this.getLocalStorageValue());
    console.log('CSS Variables:');
    const computedStyle = getComputedStyle(document.body);
    console.log('  --background-color:', computedStyle.getPropertyValue('--background-color'));
    console.log('  --primary-text:', computedStyle.getPropertyValue('--primary-text'));
    console.log('  --surface-color:', computedStyle.getPropertyValue('--surface-color'));
    console.log('==================');
  }
}
