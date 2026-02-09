import {
  ChangeDetectionStrategy,
  ChangeDetectorRef,
  Component,
  EventEmitter,
  OnInit,
  Output,
  inject,
} from '@angular/core';
import { CommonModule } from '@angular/common';
import { FormsModule } from '@angular/forms';
import { NeoButtonComponent } from '../neo/neo-button.component';
import { NeoInputComponent } from '../neo/neo-input.component';
import { AuthService, type LockoutStatus } from '../../services/auth.service';

@Component({
  selector: 'app-unlock-screen',
  standalone: true,
  imports: [CommonModule, FormsModule, NeoButtonComponent, NeoInputComponent],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="flex items-center justify-center h-screen bg-sys-bg-secondary" style="--wails-draggable: drag">
      <div class="w-full max-w-sm p-8 bg-sys-bg border-2 border-sys-border shadow-neo-lg" style="--wails-draggable: no-drag">
        <!-- Logo & Title -->
        <div class="text-center mb-6">
          <h1 class="text-2xl font-bold text-sys-fg mb-1">NS-Drive</h1>
          <p class="text-sm text-sys-fg-muted">
            @if (isSetupMode) {
              Set a master password to protect your data
            } @else {
              Enter your password to unlock
            }
          </p>
        </div>

        <!-- Error Message -->
        @if (errorMessage) {
          <div class="mb-4 p-3 bg-sys-accent-danger/20 border-2 border-sys-border text-sm text-sys-fg">
            {{ errorMessage }}
          </div>
        }

        <!-- Lockout Warning -->
        @if (lockoutStatus?.is_locked) {
          <div class="mb-4 p-3 bg-sys-accent-warning/30 border-2 border-sys-border text-sm">
            <i class="pi pi-lock mr-1"></i>
            Too many attempts. Try again in {{ lockoutStatus!.retry_after_secs }}s.
          </div>
        }

        <!-- Unlock Form -->
        @if (!isSetupMode) {
          <form (ngSubmit)="onUnlock()" class="space-y-4">
            <neo-input
              label="Password"
              type="password"
              placeholder="Enter password"
              [(ngModel)]="password"
              [disabled]="isLoading || (lockoutStatus?.is_locked ?? false)"
              name="password"
            ></neo-input>

            <neo-button
              type="submit"
              [fullWidth]="true"
              [loading]="isLoading"
              [disabled]="!password || (lockoutStatus?.is_locked ?? false)"
            >
              Unlock
            </neo-button>

            @if (lockoutStatus && lockoutStatus.failed_attempts > 0 && !lockoutStatus.is_locked) {
              <p class="text-xs text-sys-fg-muted text-center mt-2">
                {{ lockoutStatus.failed_attempts }} failed attempt(s)
              </p>
            }
          </form>
        }

        <!-- Setup Form -->
        @if (isSetupMode) {
          <form (ngSubmit)="onSetup()" class="space-y-4">
            <neo-input
              label="Password"
              type="password"
              placeholder="Choose a password"
              [(ngModel)]="password"
              [disabled]="isLoading"
              [error]="passwordError"
              name="password"
            ></neo-input>

            <neo-input
              label="Confirm Password"
              type="password"
              placeholder="Confirm password"
              [(ngModel)]="confirmPassword"
              [disabled]="isLoading"
              [error]="confirmError"
              name="confirmPassword"
            ></neo-input>

            <neo-button
              type="submit"
              [fullWidth]="true"
              [loading]="isLoading"
              [disabled]="!password || !confirmPassword"
            >
              Set Password & Encrypt
            </neo-button>

            <neo-button
              variant="ghost"
              [fullWidth]="true"
              (onClick)="onSkipSetup()"
            >
              Skip (no password)
            </neo-button>
          </form>
        }
      </div>
    </div>
  `,
})
export class UnlockScreenComponent implements OnInit {
  @Output() unlocked = new EventEmitter<void>();
  @Output() skipped = new EventEmitter<void>();

  private readonly authService = inject(AuthService);
  private readonly cdr = inject(ChangeDetectorRef);

  isSetupMode = false;
  password = '';
  confirmPassword = '';
  errorMessage = '';
  passwordError = '';
  confirmError = '';
  isLoading = false;
  lockoutStatus: LockoutStatus | null = null;

  private lockoutTimer: ReturnType<typeof setInterval> | null = null;

  async ngOnInit(): Promise<void> {
    // Self-initialize: determine setup vs unlock mode from auth state
    const isSetup = !this.authService.authEnabled$.value;
    this.isSetupMode = isSetup;
    if (!isSetup) {
      await this.refreshLockoutStatus();
    }
    this.cdr.markForCheck();
  }

  async onUnlock(): Promise<void> {
    if (!this.password || this.isLoading) return;
    if (this.lockoutStatus?.is_locked) return;

    this.isLoading = true;
    this.errorMessage = '';
    this.cdr.markForCheck();

    try {
      await this.authService.unlock(this.password);
      this.password = '';
      this.unlocked.emit();
    } catch (err) {
      this.errorMessage = this.extractErrorMessage(err);
      this.password = '';
      await this.refreshLockoutStatus();
    } finally {
      this.isLoading = false;
      this.cdr.markForCheck();
    }
  }

  async onSetup(): Promise<void> {
    if (!this.password || !this.confirmPassword || this.isLoading) return;

    this.passwordError = '';
    this.confirmError = '';
    this.errorMessage = '';

    if (this.password.length < 4) {
      this.passwordError = 'Password must be at least 4 characters';
      this.cdr.markForCheck();
      return;
    }

    if (this.password !== this.confirmPassword) {
      this.confirmError = 'Passwords do not match';
      this.cdr.markForCheck();
      return;
    }

    this.isLoading = true;
    this.cdr.markForCheck();

    try {
      await this.authService.setupPassword(this.password);
      this.password = '';
      this.confirmPassword = '';
      this.unlocked.emit();
    } catch (err) {
      this.errorMessage = this.extractErrorMessage(err);
    } finally {
      this.isLoading = false;
      this.cdr.markForCheck();
    }
  }

  onSkipSetup(): void {
    this.skipped.emit();
  }

  private async refreshLockoutStatus(): Promise<void> {
    try {
      this.lockoutStatus = await this.authService.getLockoutStatus();
      if (this.lockoutStatus?.is_locked && this.lockoutStatus.retry_after_secs > 0) {
        this.startLockoutCountdown();
      }
    } catch {
      // ignore
    }
    this.cdr.markForCheck();
  }

  private startLockoutCountdown(): void {
    this.clearLockoutTimer();
    this.lockoutTimer = setInterval(async () => {
      if (this.lockoutStatus && this.lockoutStatus.retry_after_secs > 1) {
        this.lockoutStatus = {
          ...this.lockoutStatus,
          retry_after_secs: this.lockoutStatus.retry_after_secs - 1,
        };
        this.cdr.markForCheck();
      } else {
        this.clearLockoutTimer();
        await this.refreshLockoutStatus();
      }
    }, 1000);
  }

  private clearLockoutTimer(): void {
    if (this.lockoutTimer) {
      clearInterval(this.lockoutTimer);
      this.lockoutTimer = null;
    }
  }

  private extractErrorMessage(err: unknown): string {
    if (!err) return 'An unknown error occurred';
    // Wails errors come as strings like: Error: {"message":"...","cause":{},"kind":"RuntimeError"}
    const raw = String(err);
    try {
      // Try to parse JSON from the string (strip "Error: " prefix if present)
      const jsonStr = raw.replace(/^Error:\s*/, '');
      const parsed = JSON.parse(jsonStr);
      if (parsed?.message) return parsed.message;
    } catch {
      // Not JSON — use as-is but strip "Error: " prefix
      if (raw.startsWith('Error: ')) return raw.slice(7);
    }
    return raw;
  }
}
