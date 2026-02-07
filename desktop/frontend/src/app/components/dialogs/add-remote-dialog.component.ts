import { Component, Input, Output, EventEmitter, inject } from '@angular/core';
import { CommonModule } from '@angular/common';
import { FormsModule } from '@angular/forms';
import { AppService } from '../../app.service';
import { NeoDialogComponent } from '../neo/neo-dialog.component';
import { NeoButtonComponent } from '../neo/neo-button.component';
import { NeoInputComponent } from '../neo/neo-input.component';
import { NeoDropdownComponent, DropdownOption } from '../neo/neo-dropdown.component';

@Component({
  selector: 'app-add-remote-dialog',
  standalone: true,
  imports: [
    CommonModule,
    FormsModule,
    NeoDialogComponent,
    NeoButtonComponent,
    NeoInputComponent,
    NeoDropdownComponent,
  ],
  template: `
    <neo-dialog
      [visible]="visible"
      (visibleChange)="onVisibleChange($event)"
      title="Add Remote"
      maxWidth="400px"
    >
      <div class="space-y-4">
        <!-- Label -->
        <neo-input
          label="Label"
          placeholder="My Google Drive"
          [(ngModel)]="label"
          [error]="labelError"
        ></neo-input>

        <!-- Provider -->
        <neo-dropdown
          label="Provider"
          [options]="providerOptions"
          [fullWidth]="true"
          [(ngModel)]="provider"
        ></neo-dropdown>

        <!-- Auth Status -->
        @if (authStatus) {
          <div
            class="p-3 border-2 border-sys-border text-sm"
            [class.bg-sys-accent-success/30]="authStatus === 'success'"
            [class.bg-sys-accent-danger/30]="authStatus === 'error'"
            [class.bg-sys-accent-secondary/30]="authStatus === 'pending'"
          >
            @switch (authStatus) {
              @case ('pending') {
                <i class="pi pi-spin pi-spinner mr-2"></i>
                Authenticating...
              }
              @case ('success') {
                <i class="pi pi-check-circle mr-2"></i>
                Authentication successful!
              }
              @case ('error') {
                <i class="pi pi-times-circle mr-2"></i>
                {{ authError }}
              }
            }
          </div>
        }

        <!-- Actions -->
        <div class="flex justify-end gap-2 pt-2">
          <neo-button variant="secondary" (onClick)="close()">
            Cancel
          </neo-button>

          @if (!isAuthenticated) {
            <neo-button
              [disabled]="!label || !provider"
              [loading]="authStatus === 'pending'"
              (onClick)="authenticate()"
            >
              <i class="pi pi-lock mr-1"></i> Authenticate
            </neo-button>
          } @else {
            <neo-button (onClick)="create()">
              <i class="pi pi-plus mr-1"></i> Create
            </neo-button>
          }
        </div>
      </div>
    </neo-dialog>
  `,
})
export class AddRemoteDialogComponent {
  @Input() visible = false;
  @Output() visibleChange = new EventEmitter<boolean>();
  @Output() created = new EventEmitter<string>();

  private readonly appService = inject(AppService);

  label = '';
  provider = '';
  labelError = '';
  authStatus: 'pending' | 'success' | 'error' | null = null;
  authError = '';
  isAuthenticated = false;

  providerOptions: DropdownOption[] = [
    { value: 'drive', label: 'Google Drive', icon: 'pi pi-google' },
    { value: 'dropbox', label: 'Dropbox', icon: 'pi pi-box' },
    { value: 'onedrive', label: 'OneDrive', icon: 'pi pi-microsoft' },
    { value: 'yandex', label: 'Yandex Disk', icon: 'pi pi-cloud' },
    { value: 'gphotos', label: 'Google Photos', icon: 'pi pi-images' },
  ];

  onVisibleChange(visible: boolean): void {
    if (!visible) {
      this.reset();
    }
    this.visibleChange.emit(visible);
  }

  close(): void {
    this.reset();
    this.visibleChange.emit(false);
  }

  private reset(): void {
    this.label = '';
    this.provider = '';
    this.labelError = '';
    this.authStatus = null;
    this.authError = '';
    this.isAuthenticated = false;
  }

  async authenticate(): Promise<void> {
    if (!this.label || !this.provider) return;

    // Validate label
    this.labelError = '';
    const remotes = this.appService.remotes$.value;
    if (remotes.some((r) => r.name === this.label)) {
      this.labelError = 'A remote with this name already exists';
      return;
    }

    this.authStatus = 'pending';

    try {
      await this.appService.addRemote({ name: this.label, type: this.provider });
      this.authStatus = 'success';
      this.isAuthenticated = true;
    } catch (err) {
      this.authStatus = 'error';
      this.authError = String(err) || 'Authentication failed';
      this.isAuthenticated = false;
    }
  }

  create(): void {
    if (!this.isAuthenticated) return;
    this.created.emit(this.label);
    this.close();
  }
}
