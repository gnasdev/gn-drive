import { Component, Output, EventEmitter, inject } from '@angular/core';
import { CommonModule } from '@angular/common';
import { NeoButtonComponent } from '../neo/neo-button.component';
import { AuthService } from '../../services/auth.service';

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

      <!-- Action Buttons -->
      <div class="flex items-center gap-1">
        @if (authService.authEnabled$ | async) {
          <neo-button
            variant="ghost"
            size="sm"
            (onClick)="onLock()"
            [loading]="locking"
          >
            <i class="pi pi-lock text-lg"></i>
          </neo-button>
        }
        <neo-button
          variant="ghost"
          size="sm"
          (onClick)="settingsClick.emit()"
        >
          <i class="pi pi-cog text-lg"></i>
        </neo-button>
      </div>
    </header>
  `,
})
export class TopbarComponent {
  readonly authService = inject(AuthService);

  @Output() settingsClick = new EventEmitter<void>();

  locking = false;

  async onLock(): Promise<void> {
    this.locking = true;
    try {
      await this.authService.lock();
    } catch (err) {
      console.error('Failed to lock:', err);
    } finally {
      this.locking = false;
    }
  }
}
