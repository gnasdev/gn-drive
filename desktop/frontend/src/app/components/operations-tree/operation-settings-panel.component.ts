import { Component, Input, Output, EventEmitter, OnInit } from '@angular/core';
import { CommonModule } from '@angular/common';
import { FormsModule } from '@angular/forms';
import { SyncConfig } from '../../models/flow.model';
import { NeoButtonComponent } from '../neo/neo-button.component';
import { NeoInputComponent } from '../neo/neo-input.component';
import { NeoToggleComponent } from '../neo/neo-toggle.component';
import { NeoDropdownComponent, DropdownOption } from '../neo/neo-dropdown.component';
import { PathBrowserComponent } from '../path-browser/path-browser.component';

interface PathEntry {
  value: string;
  mode: 'browser' | 'custom';
}

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
    PathBrowserComponent,
  ],
  template: `
    <div class="border-t-2 border-sys-border bg-sys-bg p-4 space-y-3" [class.opacity-50]="disabled" [class.pointer-events-none]="disabled">

      <!-- ===== Action & Mode ===== -->
      <div class="grid grid-cols-2 gap-4 items-end">
        <div>
          <span class="block text-xs font-bold text-sys-fg-muted mb-1">Action</span>
          <neo-dropdown
            [options]="actionOptions"
            [fullWidth]="true"
            [(ngModel)]="config.action"
            (ngModelChange)="onConfigChange()"
            [disabled]="disabled"
          ></neo-dropdown>
        </div>
        <div class="flex items-center h-10.5">
          <neo-toggle
            [(ngModel)]="config.dryRun"
            (ngModelChange)="onConfigChange()"
            label="Dry Run (preview only)"
            [disabled]="disabled"
          ></neo-toggle>
        </div>
      </div>

      <!-- ===== Performance ===== -->
      <div class="border-2 border-sys-border">
        <div class="flex items-center gap-2 px-3 py-2 bg-sys-bg-secondary border-b-2 border-sys-border border-l-4 border-l-[#268bd2]">
          <i class="pi pi-bolt text-xs text-[#268bd2]"></i>
          <span class="text-xs font-bold uppercase tracking-wide">Performance</span>
        </div>
        <div class="p-3 space-y-3">
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
          <details>
            <summary class="text-xs text-sys-fg-muted cursor-pointer select-none hover:text-sys-fg flex items-center gap-1">
              <i class="pi pi-cog text-[10px]"></i> Advanced
            </summary>
            <div class="grid grid-cols-2 gap-3 mt-3">
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
            <div class="mt-3">
              <neo-toggle
                [(ngModel)]="config.checkFirst"
                (ngModelChange)="onConfigChange()"
                label="Check First"
                [disabled]="disabled"
              ></neo-toggle>
            </div>
          </details>
        </div>
      </div>

      <!-- ===== Filtering ===== -->
      <div class="border-2 border-sys-border">
        <div class="flex items-center gap-2 px-3 py-2 bg-sys-bg-secondary border-b-2 border-sys-border border-l-4 border-l-[#6c71c4]">
          <i class="pi pi-filter text-xs text-[#6c71c4]"></i>
          <span class="text-xs font-bold uppercase tracking-wide">Filtering</span>
        </div>
        <div class="p-3 space-y-3">
          <!-- Include Paths -->
          <div>
            <span class="block text-xs font-bold text-sys-fg-muted mb-1">Include Paths</span>
            @for (entry of includedPathEntries; track $index; let i = $index) {
              <div class="flex items-center gap-2 mb-1">
                <button type="button" (click)="togglePathMode(includedPathEntries, i)"
                  class="text-xs px-2 py-1 border-2 border-sys-border shrink-0 hover:bg-sys-accent/20"
                  [disabled]="disabled">
                  {{ entry.mode === 'browser' ? 'Browse' : 'Custom' }}
                </button>
                @if (entry.mode === 'browser') {
                  <app-path-browser
                    [remoteName]="sourceRemote"
                    [path]="entry.value"
                    (pathChange)="onPathEntryChange(includedPathEntries, i, $event); syncIncludedPaths()"
                    placeholder="Select path"
                    filterMode="both"
                    [disabled]="disabled"
                    class="flex-1 min-w-0"
                  ></app-path-browser>
                } @else {
                  <input type="text" [(ngModel)]="entry.value"
                    (ngModelChange)="syncIncludedPaths()"
                    class="flex-1 min-w-0 px-3 py-1 border-2 border-sys-border shadow-neo-sm font-mono text-sm"
                    placeholder="/path or *.ext"
                    [disabled]="disabled" />
                }
                <button type="button" (click)="removePathEntry(includedPathEntries, i); syncIncludedPaths()"
                  class="text-sys-status-error shrink-0 px-1 hover:opacity-70" [disabled]="disabled">
                  <i class="pi pi-times text-xs"></i>
                </button>
              </div>
            }
            <button type="button" (click)="addPathEntry(includedPathEntries)"
              class="text-xs text-sys-fg-muted hover:text-sys-fg mt-1" [disabled]="disabled">
              <i class="pi pi-plus mr-1"></i> Add path
            </button>
          </div>
          <!-- Exclude Paths -->
          <div>
            <span class="block text-xs font-bold text-sys-fg-muted mb-1">Exclude Paths</span>
            @for (entry of excludedPathEntries; track $index; let i = $index) {
              <div class="flex items-center gap-2 mb-1">
                <button type="button" (click)="togglePathMode(excludedPathEntries, i)"
                  class="text-xs px-2 py-1 border-2 border-sys-border shrink-0 hover:bg-sys-accent/20"
                  [disabled]="disabled">
                  {{ entry.mode === 'browser' ? 'Browse' : 'Custom' }}
                </button>
                @if (entry.mode === 'browser') {
                  <app-path-browser
                    [remoteName]="sourceRemote"
                    [path]="entry.value"
                    (pathChange)="onPathEntryChange(excludedPathEntries, i, $event); syncExcludedPaths()"
                    placeholder="Select path"
                    filterMode="both"
                    [disabled]="disabled"
                    class="flex-1 min-w-0"
                  ></app-path-browser>
                } @else {
                  <input type="text" [(ngModel)]="entry.value"
                    (ngModelChange)="syncExcludedPaths()"
                    class="flex-1 min-w-0 px-3 py-1 border-2 border-sys-border shadow-neo-sm font-mono text-sm"
                    placeholder="*.tmp or node_modules/"
                    [disabled]="disabled" />
                }
                <button type="button" (click)="removePathEntry(excludedPathEntries, i); syncExcludedPaths()"
                  class="text-sys-status-error shrink-0 px-1 hover:opacity-70" [disabled]="disabled">
                  <i class="pi pi-times text-xs"></i>
                </button>
              </div>
            }
            <button type="button" (click)="addPathEntry(excludedPathEntries)"
              class="text-xs text-sys-fg-muted hover:text-sys-fg mt-1" [disabled]="disabled">
              <i class="pi pi-plus mr-1"></i> Add path
            </button>
          </div>
          <!-- Advanced Filtering -->
          <details>
            <summary class="text-xs text-sys-fg-muted cursor-pointer select-none hover:text-sys-fg flex items-center gap-1">
              <i class="pi pi-cog text-[10px]"></i> Advanced
            </summary>
            <div class="grid grid-cols-2 gap-3 mt-3">
              <!-- Min Size -->
              <div>
                <span class="block text-xs font-bold text-sys-fg-muted mb-1">Min Size</span>
                <div class="flex gap-1">
                  <input
                    type="number"
                    min="0"
                    class="flex-1 w-0 px-3 py-2 bg-sys-bg border-2 border-sys-border shadow-neo-sm font-medium text-sm placeholder:text-sys-fg-tertiary focus:outline-none focus:ring-2 focus:ring-sys-accent-secondary disabled:opacity-50"
                    placeholder="0"
                    [(ngModel)]="minSizeNum"
                    (ngModelChange)="onSizeFieldChange('minSize')"
                    [disabled]="disabled"
                  />
                  <neo-dropdown
                    [options]="sizeUnitOptions"
                    [(ngModel)]="minSizeUnit"
                    (ngModelChange)="onSizeFieldChange('minSize')"
                    [disabled]="disabled"
                  ></neo-dropdown>
                </div>
              </div>
              <!-- Max Size -->
              <div>
                <span class="block text-xs font-bold text-sys-fg-muted mb-1">Max Size</span>
                <div class="flex gap-1">
                  <input
                    type="number"
                    min="0"
                    class="flex-1 w-0 px-3 py-2 bg-sys-bg border-2 border-sys-border shadow-neo-sm font-medium text-sm placeholder:text-sys-fg-tertiary focus:outline-none focus:ring-2 focus:ring-sys-accent-secondary disabled:opacity-50"
                    placeholder="0"
                    [(ngModel)]="maxSizeNum"
                    (ngModelChange)="onSizeFieldChange('maxSize')"
                    [disabled]="disabled"
                  />
                  <neo-dropdown
                    [options]="sizeUnitOptions"
                    [(ngModel)]="maxSizeUnit"
                    (ngModelChange)="onSizeFieldChange('maxSize')"
                    [disabled]="disabled"
                  ></neo-dropdown>
                </div>
              </div>
              <!-- Max Age -->
              <div>
                <span class="block text-xs font-bold text-sys-fg-muted mb-1">Max Age</span>
                <div class="flex gap-1">
                  <input
                    type="number"
                    min="0"
                    class="flex-1 w-0 px-3 py-2 bg-sys-bg border-2 border-sys-border shadow-neo-sm font-medium text-sm placeholder:text-sys-fg-tertiary focus:outline-none focus:ring-2 focus:ring-sys-accent-secondary disabled:opacity-50"
                    placeholder="0"
                    [(ngModel)]="maxAgeNum"
                    (ngModelChange)="onAgeFieldChange('maxAge')"
                    [disabled]="disabled"
                  />
                  <neo-dropdown
                    [options]="ageUnitOptions"
                    [(ngModel)]="maxAgeUnit"
                    (ngModelChange)="onAgeFieldChange('maxAge')"
                    [disabled]="disabled"
                  ></neo-dropdown>
                </div>
              </div>
              <!-- Min Age -->
              <div>
                <span class="block text-xs font-bold text-sys-fg-muted mb-1">Min Age</span>
                <div class="flex gap-1">
                  <input
                    type="number"
                    min="0"
                    class="flex-1 w-0 px-3 py-2 bg-sys-bg border-2 border-sys-border shadow-neo-sm font-medium text-sm placeholder:text-sys-fg-tertiary focus:outline-none focus:ring-2 focus:ring-sys-accent-secondary disabled:opacity-50"
                    placeholder="0"
                    [(ngModel)]="minAgeNum"
                    (ngModelChange)="onAgeFieldChange('minAge')"
                    [disabled]="disabled"
                  />
                  <neo-dropdown
                    [options]="ageUnitOptions"
                    [(ngModel)]="minAgeUnit"
                    (ngModelChange)="onAgeFieldChange('minAge')"
                    [disabled]="disabled"
                  ></neo-dropdown>
                </div>
              </div>
              <neo-input
                label="Max Depth"
                type="number"
                placeholder="empty = no limit"
                [(ngModel)]="config.maxDepth"
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
            <div class="flex gap-6 mt-3">
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
      </div>

      <!-- ===== Safety ===== -->
      <details class="border-2 border-sys-border group">
        <summary class="flex items-center gap-2 px-3 py-2 bg-sys-bg-secondary cursor-pointer select-none hover:bg-sys-bg-secondary/80 border-l-4 border-l-[#b58900]">
          <i class="pi pi-shield text-xs text-[#b58900]"></i>
          <span class="text-xs font-bold uppercase tracking-wide">Safety</span>
          <i class="pi pi-chevron-down text-[10px] text-sys-fg-muted ml-auto transition-transform group-open:rotate-180"></i>
        </summary>
        <div class="p-3 border-t-2 border-sys-border space-y-3">
          <div class="grid grid-cols-2 gap-3">
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
          <div class="flex gap-6">
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
        </div>
      </details>

      <!-- ===== Comparison ===== -->
      <details class="border-2 border-sys-border group">
        <summary class="flex items-center gap-2 px-3 py-2 bg-sys-bg-secondary cursor-pointer select-none hover:bg-sys-bg-secondary/80 border-l-4 border-l-[#859900]">
          <i class="pi pi-check-circle text-xs text-[#859900]"></i>
          <span class="text-xs font-bold uppercase tracking-wide">Comparison</span>
          <i class="pi pi-chevron-down text-[10px] text-sys-fg-muted ml-auto transition-transform group-open:rotate-180"></i>
        </summary>
        <div class="p-3 border-t-2 border-sys-border">
          <div class="flex gap-6 flex-wrap">
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
        </div>
      </details>

      <!-- ===== Encryption ===== -->
      <details class="border-2 border-sys-border group">
        <summary class="flex items-center gap-2 px-3 py-2 bg-sys-bg-secondary cursor-pointer select-none hover:bg-sys-bg-secondary/80 border-l-4 border-l-[#dc322f]">
          <i class="pi pi-lock text-xs text-[#dc322f]"></i>
          <span class="text-xs font-bold uppercase tracking-wide">Encryption</span>
          <i class="pi pi-chevron-down text-[10px] text-sys-fg-muted ml-auto transition-transform group-open:rotate-180"></i>
        </summary>
        <div class="p-3 border-t-2 border-sys-border space-y-3">
          <div class="flex gap-6">
            <neo-toggle
              [(ngModel)]="config.encryptSource"
              (ngModelChange)="onConfigChange()"
              label="Encrypt Source"
              [disabled]="disabled"
            ></neo-toggle>
            <neo-toggle
              [(ngModel)]="config.encryptDest"
              (ngModelChange)="onConfigChange()"
              label="Encrypt Dest"
              [disabled]="disabled"
            ></neo-toggle>
          </div>
          @if (config.encryptSource || config.encryptDest) {
            <neo-input
              label="Password"
              type="password"
              placeholder="Encryption password"
              [(ngModel)]="config.encryptPassword"
              (ngModelChange)="onConfigChange()"
              [disabled]="disabled"
            ></neo-input>
            <neo-input
              label="Salt Password (optional)"
              type="password"
              placeholder="Optional second password"
              [(ngModel)]="config.encryptPassword2"
              (ngModelChange)="onConfigChange()"
              [disabled]="disabled"
            ></neo-input>
            <neo-dropdown
              label="Filename Encryption"
              [options]="filenameEncryptOptions"
              [fullWidth]="true"
              [(ngModel)]="config.encryptFilename"
              (ngModelChange)="onConfigChange()"
              [disabled]="disabled"
            ></neo-dropdown>
            <neo-toggle
              [(ngModel)]="config.encryptDirectory"
              (ngModelChange)="onConfigChange()"
              label="Encrypt Directory Names"
              [disabled]="disabled"
            ></neo-toggle>
          }
        </div>
      </details>

      <!-- ===== Sync Options (push/pull only) ===== -->
      @if (config.action === 'push' || config.action === 'pull') {
        <div class="border-2 border-sys-border">
          <div class="flex items-center gap-2 px-3 py-2 bg-sys-bg-secondary border-b-2 border-sys-border border-l-4 border-l-[#2aa198]">
            <i class="pi pi-sync text-xs text-[#2aa198]"></i>
            <span class="text-xs font-bold uppercase tracking-wide">Sync Options</span>
          </div>
          <div class="p-3">
            <neo-dropdown
              label="Delete Timing"
              [options]="deleteTimingOptions"
              [fullWidth]="true"
              [(ngModel)]="config.deleteTiming"
              (ngModelChange)="onConfigChange()"
              [disabled]="disabled"
            ></neo-dropdown>
          </div>
        </div>
      }

      <!-- ===== Bisync Options (bi/bi-resync only) ===== -->
      @if (config.action === 'bi' || config.action === 'bi-resync') {
        <div class="border-2 border-sys-border">
          <div class="flex items-center gap-2 px-3 py-2 bg-sys-bg-secondary border-b-2 border-sys-border border-l-4 border-l-[#d33682]">
            <i class="pi pi-arrows-h text-xs text-[#d33682]"></i>
            <span class="text-xs font-bold uppercase tracking-wide">Bisync Options</span>
          </div>
          <div class="p-3 space-y-3">
            <neo-dropdown
              label="Conflict Resolution"
              [options]="conflictOptions"
              [fullWidth]="true"
              [(ngModel)]="config.conflictResolution"
              (ngModelChange)="onConfigChange()"
              [disabled]="disabled"
            ></neo-dropdown>
            <details>
              <summary class="text-xs text-sys-fg-muted cursor-pointer select-none hover:text-sys-fg flex items-center gap-1">
                <i class="pi pi-cog text-[10px]"></i> Advanced
              </summary>
              <div class="grid grid-cols-2 gap-3 mt-3">
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
              <div class="flex gap-6 mt-3">
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
        </div>
      }

      <!-- ===== Schedule ===== -->
      <div class="border-2 border-sys-border">
        <div class="flex items-center gap-2 px-3 py-2 bg-sys-bg-secondary border-b-2 border-sys-border border-l-4 border-l-[#cb4b16]">
          <i class="pi pi-clock text-xs text-[#cb4b16]"></i>
          <span class="text-xs font-bold uppercase tracking-wide">Schedule</span>
        </div>
        <div class="p-3 space-y-3">
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
            <div class="flex gap-2 flex-wrap">
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

    </div>
  `,
})
export class OperationSettingsPanelComponent implements OnInit {
  @Input() config: SyncConfig = { action: 'push' };
  @Input() sourceRemote = '';
  @Input() targetRemote = '';
  @Input() scheduleEnabled = false;
  @Input() cronExpr = '';
  @Input() disabled = false;

  @Output() configChange = new EventEmitter<SyncConfig>();
  @Output() scheduleEnabledChange = new EventEmitter<boolean>();
  @Output() cronExprChange = new EventEmitter<string>();

  actionOptions: DropdownOption[] = [
    { value: 'push', label: 'Push', icon: 'pi pi-arrow-right', description: 'Source \u2192 Target. Deletes target files not in source.' },
    { value: 'pull', label: 'Pull', icon: 'pi pi-arrow-left', description: 'Target \u2192 Source. Deletes source files not in target.' },
    { value: 'bi', label: 'Bi-directional', icon: 'pi pi-arrows-h', description: 'Syncs both ways. Changes on either side propagate to the other.' },
    { value: 'bi-resync', label: 'Bi-directional (Resync)', icon: 'pi pi-refresh', description: 'Forces full re-sync. Use when sync state is lost or corrupted.' },
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

  filenameEncryptOptions: DropdownOption[] = [
    { value: 'standard', label: 'Standard' },
    { value: 'obfuscate', label: 'Obfuscate' },
    { value: 'off', label: 'Off' },
  ];

  sizeUnitOptions = [
    { value: 'k', label: 'KB' },
    { value: 'M', label: 'MB' },
    { value: 'G', label: 'GB' },
    { value: 'T', label: 'TB' },
  ];

  ageUnitOptions = [
    { value: 's', label: 'Sec' },
    { value: 'm', label: 'Min' },
    { value: 'h', label: 'Hour' },
    { value: 'd', label: 'Day' },
    { value: 'w', label: 'Week' },
    { value: 'M', label: 'Month' },
    { value: 'y', label: 'Year' },
  ];

  // Path entry arrays for per-row filtering UI
  includedPathEntries: PathEntry[] = [];
  excludedPathEntries: PathEntry[] = [];

  // Size/age split fields
  minSizeNum = '';
  minSizeUnit = 'M';
  maxSizeNum = '';
  maxSizeUnit = 'G';
  minAgeNum = '';
  minAgeUnit = 'h';
  maxAgeNum = '';
  maxAgeUnit = 'd';

  ngOnInit(): void {
    this.initSizeAgeFields();
    this.initPathEntries();
  }

  private initPathEntries(): void {
    this.includedPathEntries = (this.config.includedPaths || []).map((p) => ({
      value: p,
      mode: 'custom' as const,
    }));
    this.excludedPathEntries = (this.config.excludedPaths || []).map((p) => ({
      value: p,
      mode: 'custom' as const,
    }));
  }

  addPathEntry(entries: PathEntry[]): void {
    entries.push({ value: '', mode: 'custom' });
  }

  removePathEntry(entries: PathEntry[], index: number): void {
    entries.splice(index, 1);
  }

  togglePathMode(entries: PathEntry[], index: number): void {
    entries[index].mode = entries[index].mode === 'browser' ? 'custom' : 'browser';
  }

  onPathEntryChange(entries: PathEntry[], index: number, value: string): void {
    entries[index].value = value;
  }

  syncIncludedPaths(): void {
    this.config.includedPaths = this.includedPathEntries
      .map((e) => e.value)
      .filter((v) => v.trim());
    this.onConfigChange();
  }

  syncExcludedPaths(): void {
    this.config.excludedPaths = this.excludedPathEntries
      .map((e) => e.value)
      .filter((v) => v.trim());
    this.onConfigChange();
  }

  onConfigChange(): void {
    // Coerce numeric fields — neo-input type="number" emits strings
    const numericFields: (keyof SyncConfig)[] = [
      'parallel', 'bandwidth', 'multiThreadStreams', 'retries',
      'lowLevelRetries', 'tpsLimit', 'maxDelete', 'maxDepth',
    ];
    for (const field of numericFields) {
      const val = this.config[field];
      if (val === '' || val === undefined || val === null) {
        delete this.config[field];
      } else {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        (this.config as any)[field] = Number(val);
      }
    }
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

  onSizeFieldChange(field: 'minSize' | 'maxSize'): void {
    const num = field === 'minSize' ? this.minSizeNum : this.maxSizeNum;
    const unit = field === 'minSize' ? this.minSizeUnit : this.maxSizeUnit;
    this.config[field] = num ? `${num}${unit}` : '';
    this.onConfigChange();
  }

  onAgeFieldChange(field: 'minAge' | 'maxAge'): void {
    const num = field === 'minAge' ? this.minAgeNum : this.maxAgeNum;
    const unit = field === 'minAge' ? this.minAgeUnit : this.maxAgeUnit;
    this.config[field] = num ? `${num}${unit}` : '';
    this.onConfigChange();
  }

  private initSizeAgeFields(): void {
    const minSize = this.parseSizeValue(this.config.minSize);
    this.minSizeNum = minSize.num;
    this.minSizeUnit = minSize.unit;

    const maxSize = this.parseSizeValue(this.config.maxSize);
    this.maxSizeNum = maxSize.num;
    this.maxSizeUnit = maxSize.unit;

    const minAge = this.parseAgeValue(this.config.minAge);
    this.minAgeNum = minAge.num;
    this.minAgeUnit = minAge.unit;

    const maxAge = this.parseAgeValue(this.config.maxAge);
    this.maxAgeNum = maxAge.num;
    this.maxAgeUnit = maxAge.unit;
  }

  private parseSizeValue(val?: string): { num: string; unit: string } {
    if (!val) return { num: '', unit: 'M' };
    const match = val.match(/^(\d+\.?\d*)\s*([kMGT]?)$/i);
    if (!match) return { num: '', unit: 'M' };
    const unit = match[2];
    const normalized = unit.toLowerCase() === 'k' ? 'k' : unit.toUpperCase() || 'M';
    return { num: match[1], unit: normalized };
  }

  private parseAgeValue(val?: string): { num: string; unit: string } {
    if (!val) return { num: '', unit: 'h' };
    const match = val.match(/^(\d+\.?\d*)\s*([smhdwMy]?)$/);
    if (!match) return { num: '', unit: 'h' };
    return { num: match[1], unit: match[2] || 'h' };
  }
}
