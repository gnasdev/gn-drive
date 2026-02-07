import { Component, Input, Output, EventEmitter } from '@angular/core';
import { CommonModule } from '@angular/common';
import { FormsModule } from '@angular/forms';
import { SyncConfig } from '../../models/flow.model';
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
      <!-- Action Type & Dry Run -->
      <div class="grid grid-cols-2 gap-4">
        <div>
          <span class="block text-sm font-bold mb-1">Action</span>
          <neo-dropdown
            [options]="actionOptions"
            [fullWidth]="true"
            [(ngModel)]="config.action"
            (ngModelChange)="onConfigChange()"
            [disabled]="disabled"
          ></neo-dropdown>
        </div>
        <div>
          <span class="block text-sm font-bold mb-1">Dry Run</span>
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
        <div class="grid grid-cols-2 gap-3">
          <neo-input
            label="Parallel"
            type="number"
            placeholder="8"
            [(ngModel)]="config.parallel"
            (ngModelChange)="onConfigChange()"
            [disabled]="disabled"
          ></neo-input>
          <neo-input
            label="Bandwidth (MB/s)"
            type="number"
            placeholder="0 (unlimited)"
            [(ngModel)]="config.bandwidth"
            (ngModelChange)="onConfigChange()"
            [disabled]="disabled"
          ></neo-input>
        </div>
        <details class="mt-2">
          <summary class="text-xs text-sys-text-secondary cursor-pointer select-none hover:text-sys-text">Advanced Performance</summary>
          <div class="grid grid-cols-2 gap-3 mt-2">
            <neo-input
              label="Multi-thread Streams"
              type="number"
              placeholder="4"
              [(ngModel)]="config.multiThreadStreams"
              (ngModelChange)="onConfigChange()"
              [disabled]="disabled"
            ></neo-input>
            <neo-input
              label="Buffer Size"
              placeholder="16M"
              [(ngModel)]="config.bufferSize"
              (ngModelChange)="onConfigChange()"
              [disabled]="disabled"
            ></neo-input>
            <neo-input
              label="Retries"
              type="number"
              placeholder="3"
              [(ngModel)]="config.retries"
              (ngModelChange)="onConfigChange()"
              [disabled]="disabled"
            ></neo-input>
            <neo-input
              label="Low-level Retries"
              type="number"
              placeholder="10"
              [(ngModel)]="config.lowLevelRetries"
              (ngModelChange)="onConfigChange()"
              [disabled]="disabled"
            ></neo-input>
            <neo-input
              label="Max Duration"
              placeholder="e.g. 1h30m"
              [(ngModel)]="config.maxDuration"
              (ngModelChange)="onConfigChange()"
              [disabled]="disabled"
            ></neo-input>
            <neo-input
              label="Retries Sleep"
              placeholder="e.g. 10s"
              [(ngModel)]="config.retriesSleep"
              (ngModelChange)="onConfigChange()"
              [disabled]="disabled"
            ></neo-input>
            <neo-input
              label="TPS Limit"
              type="number"
              placeholder="0 (unlimited)"
              [(ngModel)]="config.tpsLimit"
              (ngModelChange)="onConfigChange()"
              [disabled]="disabled"
            ></neo-input>
            <neo-input
              label="Connect Timeout"
              placeholder="e.g. 30s"
              [(ngModel)]="config.connTimeout"
              (ngModelChange)="onConfigChange()"
              [disabled]="disabled"
            ></neo-input>
            <neo-input
              label="IO Timeout"
              placeholder="e.g. 5m"
              [(ngModel)]="config.ioTimeout"
              (ngModelChange)="onConfigChange()"
              [disabled]="disabled"
            ></neo-input>
            <neo-input
              label="Order By"
              placeholder="e.g. size,desc"
              [(ngModel)]="config.orderBy"
              (ngModelChange)="onConfigChange()"
              [disabled]="disabled"
            ></neo-input>
          </div>
          <div class="grid grid-cols-2 gap-3 mt-2">
            <neo-toggle
              [(ngModel)]="config.fastList"
              (ngModelChange)="onConfigChange()"
              label="Fast List"
              [disabled]="disabled"
            ></neo-toggle>
            <neo-toggle
              [(ngModel)]="config.checkFirst"
              (ngModelChange)="onConfigChange()"
              label="Check First"
              [disabled]="disabled"
            ></neo-toggle>
          </div>
        </details>
      </div>

      <!-- Filtering -->
      <div>
        <h3 class="text-sm font-bold mb-2 flex items-center gap-2">
          <i class="pi pi-filter"></i> Filtering
        </h3>
        <div class="grid grid-cols-2 gap-3">
          <label class="block">
            <span class="block text-sm font-medium mb-1">Include Paths</span>
            <textarea
              class="w-full px-3 py-2 border-2 border-sys-border shadow-neo-sm font-mono text-sm resize-none"
              rows="3"
              placeholder="/path/to/include&#10;/another/path"
              [ngModel]="includedPathsText"
              (ngModelChange)="updateIncludedPaths($event)"
              [disabled]="disabled"
            ></textarea>
          </label>
          <label class="block">
            <span class="block text-sm font-medium mb-1">Exclude Paths</span>
            <textarea
              class="w-full px-3 py-2 border-2 border-sys-border shadow-neo-sm font-mono text-sm resize-none"
              rows="3"
              placeholder="*.tmp&#10;node_modules/"
              [ngModel]="excludedPathsText"
              (ngModelChange)="updateExcludedPaths($event)"
              [disabled]="disabled"
            ></textarea>
          </label>
        </div>
        <details class="mt-2">
          <summary class="text-xs text-sys-text-secondary cursor-pointer select-none hover:text-sys-text">Advanced Filtering</summary>
          <div class="grid grid-cols-2 gap-3 mt-2">
            <neo-input
              label="Min Size"
              placeholder="e.g. 100k"
              [(ngModel)]="config.minSize"
              (ngModelChange)="onConfigChange()"
              [disabled]="disabled"
            ></neo-input>
            <neo-input
              label="Max Size"
              placeholder="e.g. 1G"
              [(ngModel)]="config.maxSize"
              (ngModelChange)="onConfigChange()"
              [disabled]="disabled"
            ></neo-input>
            <neo-input
              label="Max Age"
              placeholder="e.g. 24h, 7d"
              [(ngModel)]="config.maxAge"
              (ngModelChange)="onConfigChange()"
              [disabled]="disabled"
            ></neo-input>
            <neo-input
              label="Min Age"
              placeholder="e.g. 1h"
              [(ngModel)]="config.minAge"
              (ngModelChange)="onConfigChange()"
              [disabled]="disabled"
            ></neo-input>
            <neo-input
              label="Max Depth"
              type="number"
              placeholder="unlimited"
              [(ngModel)]="config.maxDepth"
              (ngModelChange)="onConfigChange()"
              [disabled]="disabled"
            ></neo-input>
            <neo-input
              label="Filter From File"
              placeholder="path to filter rules"
              [(ngModel)]="config.filterFromFile"
              (ngModelChange)="onConfigChange()"
              [disabled]="disabled"
            ></neo-input>
            <neo-input
              label="Exclude If Present"
              placeholder="e.g. .nosync"
              [(ngModel)]="config.excludeIfPresent"
              (ngModelChange)="onConfigChange()"
              [disabled]="disabled"
            ></neo-input>
          </div>
          <div class="grid grid-cols-2 gap-3 mt-2">
            <neo-toggle
              [(ngModel)]="config.useRegex"
              (ngModelChange)="onConfigChange()"
              label="Use Regex"
              [disabled]="disabled"
            ></neo-toggle>
            <neo-toggle
              [(ngModel)]="config.deleteExcluded"
              (ngModelChange)="onConfigChange()"
              label="Delete Excluded"
              [disabled]="disabled"
            ></neo-toggle>
          </div>
        </details>
      </div>

      <!-- Safety -->
      <details>
        <summary class="text-sm font-bold flex items-center gap-2 cursor-pointer select-none">
          <i class="pi pi-shield"></i> Safety
        </summary>
        <div class="grid grid-cols-2 gap-3 mt-2">
          <neo-input
            label="Max Delete (%)"
            type="number"
            placeholder="100"
            [(ngModel)]="config.maxDelete"
            (ngModelChange)="onConfigChange()"
            [disabled]="disabled"
          ></neo-input>
          <neo-input
            label="Max Transfer"
            placeholder="e.g. 10G"
            [(ngModel)]="config.maxTransfer"
            (ngModelChange)="onConfigChange()"
            [disabled]="disabled"
          ></neo-input>
          <neo-input
            label="Max Delete Size"
            placeholder="e.g. 1G"
            [(ngModel)]="config.maxDeleteSize"
            (ngModelChange)="onConfigChange()"
            [disabled]="disabled"
          ></neo-input>
          <neo-input
            label="Suffix"
            placeholder="e.g. .bak"
            [(ngModel)]="config.suffix"
            (ngModelChange)="onConfigChange()"
            [disabled]="disabled"
          ></neo-input>
          <neo-input
            label="Backup Path"
            placeholder="path for backups"
            [(ngModel)]="config.backupPath"
            (ngModelChange)="onConfigChange()"
            [disabled]="disabled"
          ></neo-input>
        </div>
        <div class="grid grid-cols-2 gap-3 mt-2">
          <neo-toggle
            [(ngModel)]="config.immutable"
            (ngModelChange)="onConfigChange()"
            label="Immutable"
            [disabled]="disabled"
          ></neo-toggle>
          <neo-toggle
            [(ngModel)]="config.suffixKeepExtension"
            (ngModelChange)="onConfigChange()"
            label="Suffix Keep Extension"
            [disabled]="disabled"
          ></neo-toggle>
        </div>
      </details>

      <!-- Comparison -->
      <details>
        <summary class="text-sm font-bold flex items-center gap-2 cursor-pointer select-none">
          <i class="pi pi-check-circle"></i> Comparison
        </summary>
        <div class="grid grid-cols-3 gap-3 mt-2">
          <neo-toggle
            [(ngModel)]="config.sizeOnly"
            (ngModelChange)="onConfigChange()"
            label="Size Only"
            [disabled]="disabled"
          ></neo-toggle>
          <neo-toggle
            [(ngModel)]="config.updateMode"
            (ngModelChange)="onConfigChange()"
            label="Update (skip newer)"
            [disabled]="disabled"
          ></neo-toggle>
          <neo-toggle
            [(ngModel)]="config.ignoreExisting"
            (ngModelChange)="onConfigChange()"
            label="Ignore Existing"
            [disabled]="disabled"
          ></neo-toggle>
        </div>
      </details>

      <!-- Sync-specific (push/pull) -->
      @if (config.action === 'push' || config.action === 'pull') {
        <details>
          <summary class="text-sm font-bold flex items-center gap-2 cursor-pointer select-none">
            <i class="pi pi-sync"></i> Sync Options
          </summary>
          <div class="mt-2">
            <neo-dropdown
              label="Delete Timing"
              [options]="deleteTimingOptions"
              [fullWidth]="true"
              [(ngModel)]="config.deleteTiming"
              (ngModelChange)="onConfigChange()"
              [disabled]="disabled"
            ></neo-dropdown>
          </div>
        </details>
      }

      <!-- Bisync Options -->
      @if (config.action === 'bi' || config.action === 'bi-resync') {
        <div>
          <h3 class="text-sm font-bold mb-2 flex items-center gap-2">
            <i class="pi pi-arrows-h"></i> Bisync Options
          </h3>
          <neo-dropdown
            label="Conflict Resolution"
            [options]="conflictOptions"
            [fullWidth]="true"
            [(ngModel)]="config.conflictResolution"
            (ngModelChange)="onConfigChange()"
            [disabled]="disabled"
          ></neo-dropdown>
          <details class="mt-2">
            <summary class="text-xs text-sys-text-secondary cursor-pointer select-none hover:text-sys-text">Advanced Bisync</summary>
            <div class="grid grid-cols-2 gap-3 mt-2">
              <neo-dropdown
                label="Conflict Loser"
                [options]="conflictLoserOptions"
                [fullWidth]="true"
                [(ngModel)]="config.conflictLoser"
                (ngModelChange)="onConfigChange()"
                [disabled]="disabled"
              ></neo-dropdown>
              <neo-input
                label="Conflict Suffix"
                placeholder="e.g. .conflict"
                [(ngModel)]="config.conflictSuffix"
                (ngModelChange)="onConfigChange()"
                [disabled]="disabled"
              ></neo-input>
              <neo-input
                label="Max Lock"
                placeholder="e.g. 15m"
                [(ngModel)]="config.maxLock"
                (ngModelChange)="onConfigChange()"
                [disabled]="disabled"
              ></neo-input>
            </div>
            <div class="grid grid-cols-2 gap-3 mt-2">
              <neo-toggle
                [(ngModel)]="config.resilient"
                (ngModelChange)="onConfigChange()"
                label="Resilient"
                [disabled]="disabled"
              ></neo-toggle>
              <neo-toggle
                [(ngModel)]="config.checkAccess"
                (ngModelChange)="onConfigChange()"
                label="Check Access"
                [disabled]="disabled"
              ></neo-toggle>
            </div>
          </details>
        </div>
      }

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

  conflictLoserOptions: DropdownOption[] = [
    { value: 'delete', label: 'Delete' },
    { value: 'num', label: 'Number suffix' },
    { value: 'pathname', label: 'Path name suffix' },
  ];

  deleteTimingOptions: DropdownOption[] = [
    { value: '', label: 'Default (during)' },
    { value: 'before', label: 'Before sync' },
    { value: 'during', label: 'During sync' },
    { value: 'after', label: 'After sync' },
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
