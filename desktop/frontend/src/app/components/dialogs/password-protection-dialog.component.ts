/** GN Drive note: Manages password protection in a dedicated GN Drive dialog. */
import { CommonModule } from '@angular/common';
import {
  ChangeDetectionStrategy,
  ChangeDetectorRef,
  Component,
  EventEmitter,
  inject,
  Input,
  OnInit,
  Output,
} from '@angular/core';
import { FormsModule } from '@angular/forms';
import { MessageService } from 'primeng/api';
import { AuthService } from '../../services/auth.service';
import { NeoButtonComponent } from '../neo/neo-button.component';
import { NeoCardComponent } from '../neo/neo-card.component';
import { NeoDialogComponent } from '../neo/neo-dialog.component';
import { NeoInputComponent } from '../neo/neo-input.component';

@Component({
  selector: 'app-password-protection-dialog',
  standalone: true,
  imports: [
    CommonModule,
    FormsModule,
    NeoDialogComponent,
    NeoButtonComponent,
    NeoCardComponent,
    NeoInputComponent,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <neo-dialog
      [visible]="visible"
      (visibleChange)="visibleChange.emit($event)"
      title="Password Protection"
      maxWidth="520px"
      [headerYellow]="true"
    >
      <div class="space-y-4">
        <neo-card>
          <div class="flex items-center gap-2 mb-3">
            <i class="pi pi-shield"></i>
            <h2 class="font-bold">Security</h2>
          </div>
          <div class="space-y-3">
            @if (authEnabled) {
              <div class="flex items-center justify-between gap-4">
                <div>
                  <p class="text-sm font-medium">Password Protection</p>
                  <p class="text-xs text-sys-fg-muted">Your data is encrypted</p>
                </div>
                <div class="flex gap-2">
                  <neo-button variant="secondary" size="sm" (onClick)="showChangePasswordDialog = true">
                    Change
                  </neo-button>
                  <neo-button variant="danger" size="sm" (onClick)="showRemovePasswordDialog = true">
                    Remove
                  </neo-button>
                </div>
              </div>
            } @else {
              <div class="flex items-center justify-between gap-4">
                <div>
                  <p class="text-sm font-medium">Password Protection</p>
                  <p class="text-xs text-sys-fg-muted">Encrypt your data with a password</p>
                </div>
                <neo-button variant="secondary" size="sm" (onClick)="showSetPasswordDialog = true">
                  Set Password
                </neo-button>
              </div>
            }
          </div>
        </neo-card>
      </div>
    </neo-dialog>

    <!-- Set Password Sub-Dialog -->
    <neo-dialog
      [(visible)]="showSetPasswordDialog"
      title="Set Password"
      maxWidth="400px"
    >
      <form (ngSubmit)="doSetPassword()" class="space-y-4">
        @if (securityError) {
          <div class="p-3 bg-sys-accent-danger/20 border-2 border-sys-border text-sm">
            {{ securityError }}
          </div>
        }
        <neo-input
          label="Password"
          type="password"
          placeholder="Choose a password"
          [(ngModel)]="newPassword"
          [error]="newPasswordError"
          name="newPassword"
        ></neo-input>
        <neo-input
          label="Confirm Password"
          type="password"
          placeholder="Confirm password"
          [(ngModel)]="confirmNewPassword"
          [error]="confirmNewPasswordError"
          name="confirmNewPassword"
        ></neo-input>
        <div class="flex justify-end gap-2 pt-2">
          <neo-button variant="secondary" (onClick)="closeSecurityDialogs()">Cancel</neo-button>
          <neo-button type="submit" [loading]="isSecurityLoading" [disabled]="!newPassword || !confirmNewPassword">
            Set Password
          </neo-button>
        </div>
      </form>
    </neo-dialog>

    <!-- Change Password Sub-Dialog -->
    <neo-dialog
      [(visible)]="showChangePasswordDialog"
      title="Change Password"
      maxWidth="400px"
    >
      <form (ngSubmit)="doChangePassword()" class="space-y-4">
        @if (securityError) {
          <div class="p-3 bg-sys-accent-danger/20 border-2 border-sys-border text-sm">
            {{ securityError }}
          </div>
        }
        <neo-input
          label="Current Password"
          type="password"
          placeholder="Enter current password"
          [(ngModel)]="currentPassword"
          name="currentPassword"
        ></neo-input>
        <neo-input
          label="New Password"
          type="password"
          placeholder="Choose new password"
          [(ngModel)]="newPassword"
          [error]="newPasswordError"
          name="newPassword"
        ></neo-input>
        <neo-input
          label="Confirm New Password"
          type="password"
          placeholder="Confirm new password"
          [(ngModel)]="confirmNewPassword"
          [error]="confirmNewPasswordError"
          name="confirmNewPassword"
        ></neo-input>
        <div class="flex justify-end gap-2 pt-2">
          <neo-button variant="secondary" (onClick)="closeSecurityDialogs()">Cancel</neo-button>
          <neo-button type="submit" [loading]="isSecurityLoading" [disabled]="!currentPassword || !newPassword || !confirmNewPassword">
            Change Password
          </neo-button>
        </div>
      </form>
    </neo-dialog>

    <!-- Remove Password Sub-Dialog -->
    <neo-dialog
      [(visible)]="showRemovePasswordDialog"
      title="Remove Password"
      maxWidth="400px"
    >
      <form (ngSubmit)="doRemovePassword()" class="space-y-4">
        @if (securityError) {
          <div class="p-3 bg-sys-accent-danger/20 border-2 border-sys-border text-sm">
            {{ securityError }}
          </div>
        }
        <p class="text-sm text-sys-fg-muted">
          This will decrypt all data and remove password protection. Enter your current password to confirm.
        </p>
        <neo-input
          label="Current Password"
          type="password"
          placeholder="Enter current password"
          [(ngModel)]="currentPassword"
          name="currentPassword"
        ></neo-input>
        <div class="flex justify-end gap-2 pt-2">
          <neo-button variant="secondary" (onClick)="closeSecurityDialogs()">Cancel</neo-button>
          <neo-button variant="danger" type="submit" [loading]="isSecurityLoading" [disabled]="!currentPassword">
            Remove Password
          </neo-button>
        </div>
      </form>
    </neo-dialog>
  `,
})
export class PasswordProtectionDialogComponent implements OnInit {
  @Input() visible = false;
  @Output() visibleChange = new EventEmitter<boolean>();

  private readonly cdr = inject(ChangeDetectorRef);
  private readonly messageService = inject(MessageService);
  private readonly authService = inject(AuthService);

  authEnabled = false;
  showSetPasswordDialog = false;
  showChangePasswordDialog = false;
  showRemovePasswordDialog = false;
  isSecurityLoading = false;
  securityError = '';
  currentPassword = '';
  newPassword = '';
  confirmNewPassword = '';
  newPasswordError = '';
  confirmNewPasswordError = '';

  ngOnInit(): void {
    this.authEnabled = this.authService.authEnabled$.value;
  }

  closeSecurityDialogs(): void {
    this.showSetPasswordDialog = false;
    this.showChangePasswordDialog = false;
    this.showRemovePasswordDialog = false;
    this.resetSecurityFields();
    this.cdr.markForCheck();
  }

  async doSetPassword(): Promise<void> {
    this.newPasswordError = '';
    this.confirmNewPasswordError = '';
    this.securityError = '';

    if (this.newPassword.length < 4) {
      this.newPasswordError = 'Password must be at least 4 characters';
      this.cdr.markForCheck();
      return;
    }
    if (this.newPassword !== this.confirmNewPassword) {
      this.confirmNewPasswordError = 'Passwords do not match';
      this.cdr.markForCheck();
      return;
    }

    this.isSecurityLoading = true;
    this.cdr.markForCheck();

    try {
      await this.authService.setupPassword(this.newPassword);
      this.authEnabled = true;
      this.messageService.add({
        severity: 'success',
        summary: 'Password Set',
        detail: 'Your data is now encrypted',
      });
      this.closeSecurityDialogs();
    } catch (err) {
      this.securityError = this.extractErrorMessage(err);
    } finally {
      this.isSecurityLoading = false;
      this.cdr.markForCheck();
    }
  }

  async doChangePassword(): Promise<void> {
    this.newPasswordError = '';
    this.confirmNewPasswordError = '';
    this.securityError = '';

    if (this.newPassword.length < 4) {
      this.newPasswordError = 'Password must be at least 4 characters';
      this.cdr.markForCheck();
      return;
    }
    if (this.newPassword !== this.confirmNewPassword) {
      this.confirmNewPasswordError = 'Passwords do not match';
      this.cdr.markForCheck();
      return;
    }

    this.isSecurityLoading = true;
    this.cdr.markForCheck();

    try {
      await this.authService.changePassword(this.currentPassword, this.newPassword);
      this.messageService.add({
        severity: 'success',
        summary: 'Password Changed',
        detail: 'Your password has been updated',
      });
      this.closeSecurityDialogs();
    } catch (err) {
      this.securityError = this.extractErrorMessage(err);
    } finally {
      this.isSecurityLoading = false;
      this.cdr.markForCheck();
    }
  }

  async doRemovePassword(): Promise<void> {
    this.securityError = '';
    this.isSecurityLoading = true;
    this.cdr.markForCheck();

    try {
      await this.authService.removePassword(this.currentPassword);
      this.authEnabled = false;
      this.messageService.add({
        severity: 'success',
        summary: 'Password Removed',
        detail: 'Password protection has been disabled',
      });
      this.closeSecurityDialogs();
    } catch (err) {
      this.securityError = this.extractErrorMessage(err);
    } finally {
      this.isSecurityLoading = false;
      this.cdr.markForCheck();
    }
  }

  private resetSecurityFields(): void {
    this.currentPassword = '';
    this.newPassword = '';
    this.confirmNewPassword = '';
    this.securityError = '';
    this.newPasswordError = '';
    this.confirmNewPasswordError = '';
  }

  private extractErrorMessage(err: unknown): string {
    if (!err) return 'An unknown error occurred';
    const raw = String(err);
    try {
      const jsonStr = raw.replace(/^Error:\s*/, '');
      const parsed = JSON.parse(jsonStr);
      if (parsed?.message) return parsed.message;
    } catch {
      if (raw.startsWith('Error: ')) return raw.slice(7);
    }
    return raw;
  }
}
