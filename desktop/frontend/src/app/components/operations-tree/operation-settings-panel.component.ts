import { Component, Input, Output, EventEmitter } from '@angular/core';
import { CommonModule } from '@angular/common';
import { FormsModule } from '@angular/forms';
import { SyncAction, SyncConfig } from '../../models/operation.model';
import { NeoButtonComponent } from '../neo/neo-button.component';
import { NeoInputComponent } from '../neo/neo-input.component';
import { NeoToggleComponent } from '../neo/neo-toggle.component';
import { NeoDropdownComponent, DropdownOption } from '../neo/neo-dropdown.component';

@Component({
  selector: 'app-operation-settings-panel',
  standalone: true,
  imports: [
    CommonModule,
    FormsModule,
    NeoButtonComponent,
    NeoInputComponent,
    NeoToggleComponent,
    NeoDropdownComponent,
  ],
  template: `
    <div class="border-t-2 border-sys-border bg-sys-bg p-4 space-y-4" [class.opacity-50]="disabled" [class.pointer-events-none]="disabled">
      <!-- Action Type -->
      <div class="grid grid-cols-2 gap-4">
        <div>
          <label class="block text-sm font-bold mb-1">Action</label>
          <neo-dropdown
            [options]="actionOptions"
            [fullWidth]="true"
            [(ngModel)]="config.action"
            (ngModelChange)="onConfigChange()"
            [disabled]="disabled"
          ></neo-dropdown>
        </div>
        <div>
          <label class="block text-sm font-bold mb-1">Dry Run</label>
          <neo-toggle
            [(ngModel)]="config.dryRun"
            (ngModelChange)="onConfigChange()"
            label="Preview only"
            [disabled]="disabled"
          ></neo-toggle>
        </div>
      </div>

      <!-- Performance -->
      <div>
        <h3 class="text-sm font-bold mb-2 flex items-center gap-2">
          <i class="pi pi-bolt"></i> Performance
        </h3>
        <div class="grid grid-cols-3 gap-3">
          <neo-input
            label="Parallel"
            type="number"
            placeholder="8"
            [(ngModel)]="config.parallel"
            (ngModelChange)="onConfigChange()"
            [disabled]="disabled"
          ></neo-input>
          <neo-input
            label="Bandwidth"
            placeholder="10M"
            [(ngModel)]="config.bandwidth"
            (ngModelChange)="onConfigChange()"
            [disabled]="disabled"
          ></neo-input>
          <neo-input
            label="Checkers"
            type="number"
            placeholder="8"
            [(ngModel)]="config.checkers"
            (ngModelChange)="onConfigChange()"
            [disabled]="disabled"
          ></neo-input>
        </div>
      </div>

      <!-- Filters -->
      <div>
        <h3 class="text-sm font-bold mb-2 flex items-center gap-2">
          <i class="pi pi-filter"></i> Filters
        </h3>
        <div class="grid grid-cols-2 gap-3">
          <div>
            <label class="block text-sm font-medium mb-1">Include Paths</label>
            <textarea
              class="w-full px-3 py-2 border-2 border-sys-border shadow-neo-sm font-mono text-sm resize-none"
              rows="3"
              placeholder="/path/to/include&#10;/another/path"
              [ngModel]="includedPathsText"
              (ngModelChange)="updateIncludedPaths($event)"
              [disabled]="disabled"
            ></textarea>
          </div>
          <div>
            <label class="block text-sm font-medium mb-1">Exclude Paths</label>
            <textarea
              class="w-full px-3 py-2 border-2 border-sys-border shadow-neo-sm font-mono text-sm resize-none"
              rows="3"
              placeholder="*.tmp&#10;node_modules/"
              [ngModel]="excludedPathsText"
              (ngModelChange)="updateExcludedPaths($event)"
              [disabled]="disabled"
            ></textarea>
          </div>
        </div>
      </div>

      <!-- Schedule -->
      <div>
        <h3 class="text-sm font-bold mb-2 flex items-center gap-2">
          <i class="pi pi-clock"></i> Schedule
        </h3>
        <div class="flex items-center gap-4">
          <neo-toggle
            [(ngModel)]="scheduleEnabled"
            (ngModelChange)="onScheduleToggle()"
            label="Enable scheduling"
            [disabled]="disabled"
          ></neo-toggle>
          @if (scheduleEnabled) {
            <neo-input
              class="flex-1"
              placeholder="0 */6 * * * (every 6 hours)"
              [(ngModel)]="cronExpr"
              (ngModelChange)="onCronChange()"
              [disabled]="disabled"
            ></neo-input>
          }
        </div>
        @if (scheduleEnabled) {
          <div class="mt-2 flex gap-2 flex-wrap">
            @for (preset of cronPresets; track preset.value) {
              <neo-button
                variant="secondary"
                size="sm"
                (onClick)="setCronPreset(preset.value)"
                [disabled]="disabled"
              >
                {{ preset.label }}
              </neo-button>
            }
          </div>
        }
      </div>

      <!-- Conflict Resolution (for bisync) -->
      @if (config.action === 'bi' || config.action === 'bi-resync') {
        <div>
          <h3 class="text-sm font-bold mb-2 flex items-center gap-2">
            <i class="pi pi-exclamation-triangle"></i> Conflict Resolution
          </h3>
          <neo-dropdown
            [options]="conflictOptions"
            [fullWidth]="true"
            [(ngModel)]="config.conflictResolution"
            (ngModelChange)="onConfigChange()"
            [disabled]="disabled"
          ></neo-dropdown>
        </div>
      }
    </div>
  `,
})
export class OperationSettingsPanelComponent {
  @Input() config: SyncConfig = { action: 'push' };
  @Input() scheduleEnabled = false;
  @Input() cronExpr = '';
  @Input() disabled = false;

  @Output() configChange = new EventEmitter<SyncConfig>();
  @Output() scheduleEnabledChange = new EventEmitter<boolean>();
  @Output() cronExprChange = new EventEmitter<string>();

  actionOptions: DropdownOption[] = [
    { value: 'push', label: 'Push', icon: 'pi pi-arrow-right' },
    { value: 'pull', label: 'Pull', icon: 'pi pi-arrow-left' },
    { value: 'bi', label: 'Bi-directional', icon: 'pi pi-arrows-h' },
    { value: 'bi-resync', label: 'Bi-directional (Resync)', icon: 'pi pi-refresh' },
  ];

  conflictOptions: DropdownOption[] = [
    { value: 'newer', label: 'Keep newer file' },
    { value: 'older', label: 'Keep older file' },
    { value: 'larger', label: 'Keep larger file' },
    { value: 'smaller', label: 'Keep smaller file' },
    { value: 'path1', label: 'Keep source file' },
    { value: 'path2', label: 'Keep target file' },
  ];

  cronPresets = [
    { label: 'Hourly', value: '0 * * * *' },
    { label: 'Every 6h', value: '0 */6 * * *' },
    { label: 'Daily', value: '0 0 * * *' },
    { label: 'Weekly', value: '0 0 * * 0' },
  ];

  get includedPathsText(): string {
    return this.config.includedPaths?.join('\n') || '';
  }

  get excludedPathsText(): string {
    return this.config.excludedPaths?.join('\n') || '';
  }

  updateIncludedPaths(text: string): void {
    this.config.includedPaths = text.split('\n').filter((p) => p.trim());
    this.onConfigChange();
  }

  updateExcludedPaths(text: string): void {
    this.config.excludedPaths = text.split('\n').filter((p) => p.trim());
    this.onConfigChange();
  }

  onConfigChange(): void {
    this.configChange.emit({ ...this.config });
  }

  onScheduleToggle(): void {
    this.scheduleEnabledChange.emit(this.scheduleEnabled);
  }

  onCronChange(): void {
    this.cronExprChange.emit(this.cronExpr);
  }

  setCronPreset(value: string): void {
    this.cronExpr = value;
    this.onCronChange();
  }
}
