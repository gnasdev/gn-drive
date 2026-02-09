import { CommonModule, DatePipe } from '@angular/common';
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
import {
  ExportToFile,
  GetExportPreview,
  SelectExportFile,
} from '../../../../wailsjs/desktop/backend/services/exportservice';
import {
  ImportFromFile,
  SelectImportFile,
  ValidateImportFile,
  ValidateImportFileWithPassword,
} from '../../../../wailsjs/desktop/backend/services/importservice';
// Inline types matching Go structs (not generated as model classes)
interface ExportOptions {
  include_boards: boolean;
  include_remotes: boolean;
  include_settings: boolean;
  exclude_tokens: boolean;
  encrypt_password: string;
}

interface ImportOptions {
  overwrite_boards: boolean;
  overwrite_remotes: boolean;
  merge_mode: boolean;
  password?: string;
}

interface ImportPreviewSection {
  to_add: string[];
  to_update: string[];
  to_skip: string[];
  total: number;
}

interface ImportPreview {
  valid: boolean;
  encrypted: boolean;
  manifest?: { export_date: string; board_count: number; remote_count: number } | null;
  boards?: ImportPreviewSection | null;
  remotes?: ImportPreviewSection | null;
  warnings: string[];
  errors: string[];
}
import {
  GetSettings,
  SetEnabled,
  SetMinimizeToTray,
  SetMinimizeToTrayOnStartup,
  SetStartAtLogin,
} from '../../../../wailsjs/desktop/backend/services/notificationservice';
import { AuthService } from '../../services/auth.service';
import { NeoButtonComponent } from '../neo/neo-button.component';
import { NeoCardComponent } from '../neo/neo-card.component';
import { NeoDialogComponent } from '../neo/neo-dialog.component';
import { NeoInputComponent } from '../neo/neo-input.component';
import { NeoToggleComponent } from '../neo/neo-toggle.component';

@Component({
  selector: 'app-settings-dialog',
  standalone: true,
  imports: [
    CommonModule,
    FormsModule,
    DatePipe,
    NeoDialogComponent,
    NeoButtonComponent,
    NeoCardComponent,
    NeoInputComponent,
    NeoToggleComponent,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <neo-dialog
      [visible]="visible"
      (visibleChange)="visibleChange.emit($event)"
      title="Settings v1.0.0"
      maxWidth="80vw"
      maxHeight="80vh"
      [headerYellow]="true"
    >
      <div class="space-y-4 h-full overflow-auto hide-scrollbar">
        <!-- Security -->
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

        <!-- Backup & Restore -->
        <neo-card>
          <div class="flex items-center gap-2 mb-3">
            <i class="pi pi-download"></i>
            <h2 class="font-bold">Backup & Restore</h2>
          </div>
          <div class="space-y-3">
            <div class="flex items-center justify-between gap-4">
              <div>
                <p class="text-sm font-medium">Export Configuration</p>
                <p class="text-xs text-sys-fg-muted">Save boards, remotes, and settings</p>
              </div>
              <neo-button variant="secondary" size="sm" (onClick)="openExportDialog()">
                <i class="pi pi-upload mr-1"></i> Export
              </neo-button>
            </div>
            <div class="flex items-center justify-between gap-4">
              <div>
                <p class="text-sm font-medium">Import Configuration</p>
                <p class="text-xs text-sys-fg-muted">Restore from backup file</p>
              </div>
              <neo-button variant="secondary" size="sm" (onClick)="openImportDialog()">
                <i class="pi pi-download mr-1"></i> Import
              </neo-button>
            </div>
          </div>
        </neo-card>

        <!-- Startup -->
        <neo-card>
          <div class="flex items-center gap-2 mb-3">
            <i class="pi pi-power-off"></i>
            <h2 class="font-bold">Startup</h2>
          </div>
          <div class="space-y-3">
            <div class="flex items-center justify-between">
              <div>
                <p class="text-sm font-medium">Start at Login</p>
                <p class="text-xs text-sys-fg-muted">Auto-start when you log in</p>
              </div>
              <neo-toggle
                [(ngModel)]="startAtLogin"
                (ngModelChange)="saveStartAtLoginSetting()"
              ></neo-toggle>
            </div>
            <div class="flex items-center justify-between">
              <div>
                <p class="text-sm font-medium">Minimize to Tray</p>
                <p class="text-xs text-sys-fg-muted">Keep running when closed</p>
              </div>
              <neo-toggle
                [(ngModel)]="minimizeToTray"
                (ngModelChange)="saveMinimizeToTraySetting()"
              ></neo-toggle>
            </div>
            <div class="flex items-center justify-between">
              <div>
                <p class="text-sm font-medium">Minimize to Tray on Startup</p>
                <p class="text-xs text-sys-fg-muted">Start minimized to system tray</p>
              </div>
              <neo-toggle
                [(ngModel)]="minimizeToTrayOnStartup"
                (ngModelChange)="saveMinimizeToTrayOnStartupSetting()"
              ></neo-toggle>
            </div>
          </div>
        </neo-card>

        <!-- Notifications -->
        <neo-card>
          <div class="flex items-center gap-2 mb-3">
            <i class="pi pi-bell"></i>
            <h2 class="font-bold">Notifications</h2>
          </div>
          <div class="flex items-center justify-between">
            <div>
              <p class="text-sm font-medium">Desktop Notifications</p>
              <p class="text-xs text-sys-fg-muted">Show on complete/fail</p>
            </div>
            <neo-toggle
              [(ngModel)]="notificationsEnabled"
              (ngModelChange)="saveNotificationSetting()"
            ></neo-toggle>
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

    <!-- Export Sub-Dialog -->
    <neo-dialog
      [(visible)]="showExportDialog"
      title="Export Configuration"
      maxWidth="400px"
    >
      <div class="space-y-4">
        <p class="text-sm text-sys-fg-muted">Select what to include:</p>

        <div class="space-y-2">
          <label class="flex items-center gap-2 cursor-pointer">
            <input type="checkbox" [(ngModel)]="exportOptions.include_boards"
                   class="w-4 h-4 border-2 border-sys-border" />
            <span>Boards ({{ exportPreview?.board_count || 0 }})</span>
          </label>
          <label class="flex items-center gap-2 cursor-pointer">
            <input type="checkbox" [(ngModel)]="exportOptions.include_remotes"
                   class="w-4 h-4 border-2 border-sys-border" />
            <span>Remotes ({{ exportPreview?.remote_count || 0 }})</span>
          </label>
        </div>

        @if (exportOptions.include_remotes) {
          <div class="p-3 bg-sys-accent-warning/30 border-2 border-sys-border text-sm">
            <i class="pi pi-exclamation-triangle mr-1"></i>
            Remotes contain auth tokens. Keep backup secure.
          </div>

          <label class="flex items-center gap-2 cursor-pointer">
            <input type="checkbox" [(ngModel)]="exportOptions.exclude_tokens"
                   class="w-4 h-4 border-2 border-sys-border" />
            <span class="text-sm text-sys-fg-muted">Exclude tokens (re-auth required)</span>
          </label>
        }

        <!-- Encrypt export -->
        <label class="flex items-center gap-2 cursor-pointer">
          <input type="checkbox" [(ngModel)]="exportEncrypt"
                 class="w-4 h-4 border-2 border-sys-border" />
          <span class="text-sm">Encrypt backup file</span>
        </label>

        @if (exportEncrypt) {
          <neo-input
            label="Export Password"
            type="password"
            placeholder="Password for encryption"
            [(ngModel)]="exportPassword"
            [error]="exportPasswordError"
            name="exportPassword"
          ></neo-input>
          <neo-input
            label="Confirm Password"
            type="password"
            placeholder="Confirm password"
            [(ngModel)]="exportPasswordConfirm"
            [error]="exportPasswordConfirmError"
            name="exportPasswordConfirm"
          ></neo-input>
        }

        <div class="flex justify-end gap-2 pt-2">
          <neo-button variant="secondary" (onClick)="showExportDialog = false">
            Cancel
          </neo-button>
          <neo-button
            [disabled]="(!exportOptions.include_boards && !exportOptions.include_remotes) || (exportEncrypt && (!exportPassword || !exportPasswordConfirm))"
            [loading]="isExporting"
            (onClick)="doExport()"
          >
            Export
          </neo-button>
        </div>
      </div>
    </neo-dialog>

    <!-- Import Sub-Dialog -->
    <neo-dialog
      [(visible)]="showImportDialog"
      title="Import Configuration"
      maxWidth="450px"
    >
      @if (!importPreview && !importNeedsPassword) {
        <div class="text-center py-4">
          <p class="text-sm text-sys-fg-muted mb-4">Select a backup file (.nsd)</p>
          <neo-button [loading]="isLoadingPreview" (onClick)="selectImportFile()">
            <i class="pi pi-folder-open mr-1"></i> Select File
          </neo-button>
        </div>
      } @else if (importNeedsPassword) {
        <!-- Encrypted file needs password -->
        <div class="space-y-4">
          <div class="p-3 bg-sys-accent-secondary/20 border-2 border-sys-border text-sm">
            <i class="pi pi-lock mr-1"></i>
            This backup file is encrypted. Enter the password to continue.
          </div>
          @if (importPasswordError) {
            <div class="p-3 bg-sys-accent-danger/20 border-2 border-sys-border text-sm">
              {{ importPasswordError }}
            </div>
          }
          <neo-input
            label="Password"
            type="password"
            placeholder="Enter backup password"
            [(ngModel)]="importPassword"
            name="importPassword"
          ></neo-input>
          <div class="flex justify-end gap-2 pt-2">
            <neo-button variant="secondary" (onClick)="cancelImport()">Cancel</neo-button>
            <neo-button [loading]="isLoadingPreview" [disabled]="!importPassword" (onClick)="unlockImportFile()">
              Unlock
            </neo-button>
          </div>
        </div>
      } @else if (importPreview) {
        <div class="space-y-4">
          <div class="p-3 bg-sys-accent-secondary/20 border-2 border-sys-border text-sm">
            <div class="flex justify-between">
              <span class="text-sys-fg-muted">Date:</span>
              <span>{{ importPreview.manifest?.export_date | date:'medium' }}</span>
            </div>
            <div class="flex justify-between">
              <span class="text-sys-fg-muted">File:</span>
              <span class="truncate max-w-[200px]">{{ importFileName }}</span>
            </div>
          </div>

          @if (importPreview.boards) {
            <div class="p-3 border-2 border-sys-border">
              <h4 class="font-bold mb-2">Boards ({{ importPreview.boards.total }})</h4>
              <div class="text-xs space-y-1">
                @if (importPreview.boards.to_add.length) {
                  <div class="text-sys-status-success">+ {{ importPreview.boards.to_add.length }} new</div>
                }
                @if (importPreview.boards.to_update.length) {
                  <div class="text-sys-status-warning">~ {{ importPreview.boards.to_update.length }} existing</div>
                }
              </div>
            </div>
          }

          @if (importPreview.remotes) {
            <div class="p-3 border-2 border-sys-border">
              <h4 class="font-bold mb-2">Remotes ({{ importPreview.remotes.total }})</h4>
              <div class="text-xs space-y-1">
                @if (importPreview.remotes.to_add.length) {
                  <div class="text-sys-status-success">+ {{ importPreview.remotes.to_add.length }} new</div>
                }
                @if (importPreview.remotes.to_update.length) {
                  <div class="text-sys-status-warning">~ {{ importPreview.remotes.to_update.length }} existing</div>
                }
              </div>
            </div>
          }

          @if (importPreview.warnings.length) {
            <div class="p-3 bg-sys-accent-warning/30 border-2 border-sys-border text-sm">
              <div class="font-bold mb-1">Warnings:</div>
              @for (warning of importPreview.warnings; track warning) {
                <div>{{ warning }}</div>
              }
            </div>
          }

          <div class="space-y-2">
            <label class="flex items-center gap-2 cursor-pointer">
              <input type="checkbox" [(ngModel)]="importOptions.overwrite_boards"
                     class="w-4 h-4 border-2 border-sys-border" />
              <span class="text-sm">Overwrite existing boards</span>
            </label>
            <label class="flex items-center gap-2 cursor-pointer">
              <input type="checkbox" [(ngModel)]="importOptions.overwrite_remotes"
                     class="w-4 h-4 border-2 border-sys-border" />
              <span class="text-sm">Overwrite existing remotes</span>
            </label>
          </div>

          <div class="flex justify-end gap-2 pt-2">
            <neo-button variant="secondary" (onClick)="cancelImport()">
              Cancel
            </neo-button>
            <neo-button [loading]="isImporting" (onClick)="doImport()">
              Import
            </neo-button>
          </div>
        </div>
      }
    </neo-dialog>
  `,
})
export class SettingsDialogComponent implements OnInit {
  @Input() visible = false;
  @Output() visibleChange = new EventEmitter<boolean>();

  private readonly cdr = inject(ChangeDetectorRef);
  private readonly messageService = inject(MessageService);
  private readonly authService = inject(AuthService);

  // Settings state
  startAtLogin = false;
  minimizeToTray = false;
  minimizeToTrayOnStartup = false;
  notificationsEnabled = true;

  // Security state
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

  // Export dialog
  showExportDialog = false;
  isExporting = false;
  exportOptions: ExportOptions = {
    include_boards: true,
    include_remotes: true,
    include_settings: false,
    exclude_tokens: false,
    encrypt_password: '',
  };
  exportPreview: { board_count: number; remote_count: number } | null = null;
  exportEncrypt = false;
  exportPassword = '';
  exportPasswordConfirm = '';
  exportPasswordError = '';
  exportPasswordConfirmError = '';

  // Import dialog
  showImportDialog = false;
  isImporting = false;
  isLoadingPreview = false;
  importFilePath = '';
  importFileName = '';
  importPreview: ImportPreview | null = null;
  importNeedsPassword = false;
  importPassword = '';
  importPasswordError = '';
  importOptions: ImportOptions = {
    overwrite_boards: false,
    overwrite_remotes: false,
    merge_mode: false,
  };

  async ngOnInit(): Promise<void> {
    await this.loadSettings();
    this.authEnabled = this.authService.authEnabled$.value;
  }

  private async loadSettings(): Promise<void> {
    try {
      const settings = await GetSettings();
      this.startAtLogin = settings.start_at_login;
      this.minimizeToTray = settings.minimize_to_tray;
      this.minimizeToTrayOnStartup = settings.minimize_to_tray_on_startup;
      this.notificationsEnabled = settings.notifications_enabled;
      this.cdr.markForCheck();
    } catch (err) {
      console.error('Failed to load settings:', err);
    }
  }

  // --- Security methods ---

  closeSecurityDialogs(): void {
    this.showSetPasswordDialog = false;
    this.showChangePasswordDialog = false;
    this.showRemovePasswordDialog = false;
    this.resetSecurityFields();
    this.cdr.markForCheck();
  }

  private resetSecurityFields(): void {
    this.currentPassword = '';
    this.newPassword = '';
    this.confirmNewPassword = '';
    this.securityError = '';
    this.newPasswordError = '';
    this.confirmNewPasswordError = '';
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

  // --- Settings methods ---

  async saveStartAtLoginSetting(): Promise<void> {
    try {
      await SetStartAtLogin(this.startAtLogin);
    } catch (err) {
      console.error('Failed to save setting:', err);
    }
  }

  async saveMinimizeToTraySetting(): Promise<void> {
    try {
      await SetMinimizeToTray(this.minimizeToTray);
    } catch (err) {
      console.error('Failed to save setting:', err);
    }
  }

  async saveMinimizeToTrayOnStartupSetting(): Promise<void> {
    try {
      await SetMinimizeToTrayOnStartup(this.minimizeToTrayOnStartup);
    } catch (err) {
      console.error('Failed to save setting:', err);
    }
  }

  async saveNotificationSetting(): Promise<void> {
    try {
      await SetEnabled(this.notificationsEnabled);
    } catch (err) {
      console.error('Failed to save setting:', err);
    }
  }

  // --- Export/Import methods ---

  async openExportDialog(): Promise<void> {
    try {
      this.exportPreview = await GetExportPreview(this.exportOptions);
      this.showExportDialog = true;
      this.cdr.markForCheck();
    } catch (err) {
      console.error('Failed to get export preview:', err);
    }
  }

  async doExport(): Promise<void> {
    this.exportPasswordError = '';
    this.exportPasswordConfirmError = '';

    if (this.exportEncrypt) {
      if (this.exportPassword.length < 4) {
        this.exportPasswordError = 'Password must be at least 4 characters';
        this.cdr.markForCheck();
        return;
      }
      if (this.exportPassword !== this.exportPasswordConfirm) {
        this.exportPasswordConfirmError = 'Passwords do not match';
        this.cdr.markForCheck();
        return;
      }
    }

    this.isExporting = true;
    this.cdr.markForCheck();

    try {
      const filePath = await SelectExportFile();
      if (!filePath) {
        this.isExporting = false;
        this.cdr.markForCheck();
        return;
      }

      const options = {
        ...this.exportOptions,
        encrypt_password: this.exportEncrypt ? this.exportPassword : '',
      };
      await ExportToFile(filePath, options);
      this.messageService.add({
        severity: 'success',
        summary: 'Export Complete',
        detail: 'Configuration exported successfully',
      });
      this.showExportDialog = false;
      this.exportEncrypt = false;
      this.exportPassword = '';
      this.exportPasswordConfirm = '';
    } catch (err) {
      this.messageService.add({
        severity: 'error',
        summary: 'Export Failed',
        detail: this.extractErrorMessage(err),
      });
    } finally {
      this.isExporting = false;
      this.cdr.markForCheck();
    }
  }

  openImportDialog(): void {
    this.importPreview = null;
    this.importFilePath = '';
    this.importFileName = '';
    this.importNeedsPassword = false;
    this.importPassword = '';
    this.importPasswordError = '';
    this.showImportDialog = true;
    this.cdr.markForCheck();
  }

  async selectImportFile(): Promise<void> {
    this.isLoadingPreview = true;
    this.cdr.markForCheck();

    try {
      const filePath = await SelectImportFile();
      if (!filePath) {
        this.isLoadingPreview = false;
        this.cdr.markForCheck();
        return;
      }

      this.importFilePath = filePath;
      this.importFileName = filePath.split('/').pop() || filePath;
      const preview = await ValidateImportFile(filePath) as ImportPreview & { encrypted?: boolean };

      if (preview.encrypted && !preview.valid) {
        this.importNeedsPassword = true;
      } else {
        this.importPreview = preview;
      }
    } catch (err) {
      this.messageService.add({
        severity: 'error',
        summary: 'Invalid File',
        detail: this.extractErrorMessage(err),
      });
    } finally {
      this.isLoadingPreview = false;
      this.cdr.markForCheck();
    }
  }

  async unlockImportFile(): Promise<void> {
    if (!this.importPassword || !this.importFilePath) return;

    this.isLoadingPreview = true;
    this.importPasswordError = '';
    this.cdr.markForCheck();

    try {
      const preview = await ValidateImportFileWithPassword(this.importFilePath, this.importPassword);
      if (preview && preview.valid) {
        this.importPreview = preview;
        this.importNeedsPassword = false;
      } else {
        this.importPasswordError = preview?.errors?.[0] || 'Wrong password or corrupted file';
      }
    } catch (err) {
      this.importPasswordError = this.extractErrorMessage(err);
    } finally {
      this.isLoadingPreview = false;
      this.cdr.markForCheck();
    }
  }

  cancelImport(): void {
    this.importPreview = null;
    this.importNeedsPassword = false;
    this.importPassword = '';
    this.importPasswordError = '';
    this.showImportDialog = false;
    this.cdr.markForCheck();
  }

  async doImport(): Promise<void> {
    if (!this.importFilePath) return;

    this.isImporting = true;
    this.cdr.markForCheck();

    try {
      const options = {
        ...this.importOptions,
        password: this.importPassword,
      };
      await ImportFromFile(this.importFilePath, options);
      this.messageService.add({
        severity: 'success',
        summary: 'Import Complete',
        detail: 'Configuration imported successfully',
      });
      this.showImportDialog = false;
      this.importPassword = '';
    } catch (err) {
      this.messageService.add({
        severity: 'error',
        summary: 'Import Failed',
        detail: this.extractErrorMessage(err),
      });
    } finally {
      this.isImporting = false;
      this.cdr.markForCheck();
    }
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
