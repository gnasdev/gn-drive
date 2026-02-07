import { Component, Output, EventEmitter } from '@angular/core';
import { CommonModule } from '@angular/common';
import { NeoButtonComponent } from '../neo/neo-button.component';

@Component({
  selector: 'app-topbar',
  standalone: true,
  imports: [CommonModule, NeoButtonComponent],
  template: `
    <header class="h-14 bg-sys-accent border-b-2 border-sys-border px-4 flex items-center justify-between">
      <!-- App Name -->
      <div class="flex items-center gap-2">
        <i class="pi pi-cloud text-xl text-sys-fg"></i>
        <h1 class="text-xl font-bold text-sys-fg">NS-Drive</h1>
      </div>

      <!-- Settings Button -->
      <neo-button
        variant="ghost"
        size="sm"
        (onClick)="settingsClick.emit()"
      >
        <i class="pi pi-cog text-lg"></i>
      </neo-button>
    </header>
  `,
})
export class TopbarComponent {
  @Output() settingsClick = new EventEmitter<void>();
}
