/** GN Drive note: Renders a reusable GN Drive interface component. */
import { ChangeDetectionStrategy, Component, computed, input, signal } from '@angular/core';
import { CommonModule } from '@angular/common';
import { SyncStatus, FileTransferInfo } from '../../models/sync-status.interface';
import { NeoDialogComponent } from '../neo/neo-dialog.component';

type FileTabId = 'syncing' | 'check' | 'complete' | 'error';

@Component({
  selector: 'app-operation-logs-panel',
  standalone: true,
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [CommonModule, NeoDialogComponent],
  template: `
    @if (syncStatus(); as ss) {
      <div class="border-t-2 border-sys-border bg-sys-bg-inverse p-4 space-y-4">
        <!-- Progress header -->
        <div class="flex items-start justify-between gap-4">
          <div class="min-w-0">
            <div class="flex items-center gap-2">
              <i [class]="statusHeaderIcon()" class="text-sm"></i>
              <span class="font-bold text-base text-sys-fg-inverse">{{ detailedStatusLabel() }}</span>
              <span class="text-xs font-bold uppercase text-sys-fg-muted">{{ actionLabel() }}</span>
            </div>
            <p class="mt-1 text-xs text-sys-fg-muted truncate" [title]="backendDetail()">
              {{ backendDetail() }}
            </p>
          </div>
          <div class="flex items-center gap-3 shrink-0">
            <span class="font-bold text-lg text-sys-fg-inverse">{{ ss.progress.toFixed(1) }}%</span>
            <button
              type="button"
              class="inline-flex size-7 items-center justify-center bg-sys-bg/10 text-sys-fg-inverse hover:bg-sys-bg/20 transition-colors"
              [attr.aria-expanded]="isExpanded()"
              aria-label="Toggle sync status details"
              (click)="toggleExpanded()"
            >
              <i [class]="isExpanded() ? 'pi pi-chevron-up' : 'pi pi-chevron-down'" class="text-xs"></i>
            </button>
          </div>
        </div>

        @if (!isExpanded()) {
          <div class="space-y-2">
            <div class="flex items-center justify-between gap-4 text-xs">
              <span class="font-bold uppercase text-sys-fg-muted shrink-0">Current path</span>
              <span class="min-w-0 truncate text-right text-sys-fg-inverse" [title]="currentPathLabel()">
                {{ currentPathLabel() }}
              </span>
            </div>
            <div class="w-full h-2 bg-sys-bg-tertiary">
              <div
                class="h-full transition-all duration-300"
                [class]="progressBarClass()"
                [style.width.%]="ss.progress"
              ></div>
            </div>
          </div>
        } @else {
          <!-- Progress bar -->
          <div class="w-full h-3 bg-sys-bg-tertiary border-2 border-sys-border">
            <div
              class="h-full transition-all duration-300"
              [class]="progressBarClass()"
              [style.width.%]="ss.progress"
            ></div>
          </div>

          <!-- Backend progress details -->
          <div class="grid grid-cols-4 gap-2 text-xs">
            <div class="border-2 border-sys-border bg-sys-bg-secondary px-2 py-1">
              <div class="font-bold text-sys-fg-muted uppercase">Files</div>
              <div class="font-bold text-sys-fg">{{ ss.files_transferred }}@if (ss.total_files > 0) { / {{ ss.total_files }}}</div>
            </div>
            <div class="border-2 border-sys-border bg-sys-bg-secondary px-2 py-1">
              <div class="font-bold text-sys-fg-muted uppercase">Checks</div>
              <div class="font-bold text-sys-fg">{{ ss.checks }}@if (ss.total_checks > 0) { / {{ ss.total_checks }}}</div>
            </div>
            <div class="border-2 border-sys-border bg-sys-bg-secondary px-2 py-1">
              <div class="font-bold text-sys-fg-muted uppercase">Deletes</div>
              <div class="font-bold text-sys-fg">{{ ss.deletes }}</div>
            </div>
            <div class="border-2 border-sys-border bg-sys-bg-secondary px-2 py-1">
              <div class="font-bold text-sys-fg-muted uppercase">Renames</div>
              <div class="font-bold text-sys-fg">{{ ss.renames }}</div>
            </div>
          </div>

          <!-- Runtime summary -->
          <div class="flex items-center justify-between gap-4 text-xs">
            <div class="flex items-center gap-2 text-sys-fg-inverse font-medium">
              <i class="pi pi-bolt text-xs"></i>
              <span>{{ ss.speed }}</span>
            </div>
            <div class="flex items-center justify-center gap-2 text-sys-fg-inverse font-medium">
              <i class="pi pi-database text-xs"></i>
              <span>{{ formatBytes(ss.bytes_transferred) }}@if (ss.total_bytes > 0) { / {{ formatBytes(ss.total_bytes) }}}</span>
            </div>
            <div class="flex items-center justify-end gap-2 text-sys-fg-inverse font-medium">
              <i class="pi pi-stopwatch text-xs"></i>
              <span>ETA {{ ss.eta && ss.eta !== '--' ? ss.eta : 'calculating' }}</span>
            </div>
          </div>

          <!-- File Transfers List -->
          @if (sortedTransfers().length > 0) {
            <div class="border-t-2 border-sys-border pt-3">
              <div class="mb-2">
                <div
                  class="grid grid-cols-4 gap-1 bg-sys-bg/10 p-1"
                  role="tablist"
                  aria-label="File transfer status"
                >
                  @for (tab of fileTabs; track tab.id) {
                    <button
                      [id]="'file-tab-' + tab.id"
                      type="button"
                      role="tab"
                      class="min-w-0 px-2 py-1.5 text-xs font-bold transition-colors flex items-center justify-center gap-1"
                      [class]="getFileTabButtonClass(tab.id)"
                      [attr.aria-selected]="selectedFileTab() === tab.id"
                      [attr.aria-controls]="'file-tab-panel-' + tab.id"
                      [attr.tabindex]="selectedFileTab() === tab.id ? 0 : -1"
                      [class.opacity-70]="fileTabCounts()[tab.id] === 0 && selectedFileTab() !== tab.id"
                      (click)="setFileTab(tab.id)"
                      (keydown.arrowright)="selectNextFileTab()"
                      (keydown.arrowleft)="selectPreviousFileTab()"
                    >
                      <i [class]="tab.icon" class="shrink-0"></i>
                      <span class="truncate">{{ tab.label }}</span>
                      <span class="min-w-5 px-1 py-0.5 text-[10px] leading-none bg-sys-bg-inverse/20 text-center">
                        {{ fileTabCounts()[tab.id] }}
                      </span>
                    </button>
                  }
                </div>
              </div>
              <div
                [id]="'file-tab-panel-' + selectedFileTab()"
                role="tabpanel"
                [attr.aria-labelledby]="'file-tab-' + selectedFileTab()"
                class="max-h-48 overflow-auto space-y-1"
              >
                @for (file of selectedTransfers(); track file.name + file.status) {
                  <div class="flex items-center gap-2 px-2 py-1.5 text-sm rounded"
                       [class]="getFileRowClass(file)"
                       [style.background]="getFileRowBackground(file)">
                    @if (selectedFileTab() !== 'syncing') {
                      <i [class]="getFileStatusIcon(file)" class="text-sm w-4 flex-shrink-0"></i>
                    }
                    <span class="flex-1 min-w-0 truncate font-medium text-sys-fg-inverse" [title]="file.name">
                      {{ getFileName(file.name) }}
                    </span>
                    @if (file.status === 'transferring') {
                      <span class="text-sys-status-warning font-bold flex-shrink-0">{{ file.progress.toFixed(0) }}%</span>
                      @if (file.speed) {
                        <span class="text-sys-fg-inverse text-xs flex-shrink-0">{{ formatSpeed(file.speed) }}</span>
                      }
                    } @else {
                      <span class="text-sys-fg-muted text-xs flex-shrink-0">{{ formatBytes(file.bytes || file.size) }}</span>
                    }
                    @if (file.error) {
                      <button
                        class="text-sys-status-error text-xs flex-shrink-0 hover:opacity-70 cursor-pointer"
                        (click)="showError(file.error)"
                        [title]="file.error"
                      >
                        <i class="pi pi-exclamation-circle"></i>
                      </button>
                    }
                  </div>
                } @empty {
                  <div class="px-2 py-3 text-sm text-sys-fg-muted">
                    No files in this tab.
                  </div>
                }
              </div>
            </div>
          }
        }
      </div>
    }

    <!-- Error Detail Dialog -->
    <neo-dialog
      [(visible)]="showErrorDialog"
      title="Error Details"
      maxWidth="600px"
    >
      <pre class="text-sm text-sys-status-error whitespace-pre-wrap break-all font-mono">{{ selectedError }}</pre>
    </neo-dialog>

  `,
})
export class OperationLogsPanelComponent {
  readonly syncStatus = input<SyncStatus | null>(null);
  readonly isExpanded = signal(false);
  readonly selectedFileTab = signal<FileTabId>('syncing');
  readonly fileTabs = [
    { id: 'syncing', label: 'Syncing', icon: 'pi pi-spin pi-spinner' },
    { id: 'check', label: 'Check', icon: 'pi pi-search' },
    { id: 'complete', label: 'Complete', icon: 'pi pi-check-circle' },
    { id: 'error', label: 'Error', icon: 'pi pi-exclamation-circle' },
  ] as const;

  selectedError: string | null = null;
  showErrorDialog = false;
  readonly progressBarClass = computed(() => {
    const s = this.syncStatus();
    if (!s) return 'bg-sys-status-success';
    switch (s.status) {
      case 'completed':
        return 'bg-sys-status-success';
      case 'error':
        return 'bg-sys-status-error';
      case 'stopped':
        return 'bg-sys-fg-muted';
      default:
        return 'bg-sys-status-success';
    }
  });

  readonly latestBackendLog = computed(() => {
    const messages = this.syncStatus()?.log_messages;
    return messages?.length ? messages[messages.length - 1] : '';
  });

  readonly detailedStatusLabel = computed(() => {
    const s = this.syncStatus();
    if (!s) return 'Waiting';
    if (s.status !== 'running') return this.toTitleCase(s.status);
    if (this.fileGroups().syncing.length > 0 || s.bytes_transferred > 0) return 'Transferring';
    if (s.checks > 0 || s.total_checks > 0) return 'Checking';
    if (this.latestBackendLog()) return 'Resolving';
    return 'Preparing';
  });

  readonly backendDetail = computed(() => {
    const s = this.syncStatus();
    if (!s) return 'Waiting for backend progress...';
    const latestLog = this.latestBackendLog();
    if (latestLog) return latestLog;
    if (s.current_file) return s.current_file;
    if (s.delta_skipped) return 'No changes detected; delta sync skipped this operation.';
    if (s.total_checks > 0) return `Checking ${s.checks} of ${s.total_checks} items.`;
    if (s.total_files > 0) return `Transferred ${s.files_transferred} of ${s.total_files} files.`;
    if (s.bytes_transferred > 0) return `Transferred ${this.formatBytes(s.bytes_transferred)}.`;
    return 'Backend is preparing file list and resolving remotes.';
  });

  readonly currentPathLabel = computed(() => {
    const s = this.syncStatus();
    if (!s) return 'Waiting for backend progress...';
    if (s.current_file) return s.current_file;
    const syncingFile = this.fileGroups().syncing[0];
    if (syncingFile) return syncingFile.name;
    return this.backendDetail();
  });

  readonly actionLabel = computed(() => {
    const action = this.syncStatus()?.action;
    switch (action) {
      case 'pull':
        return 'Pull';
      case 'push':
        return 'Push';
      case 'bi':
        return 'Bi-sync';
      case 'bi-resync':
        return 'Bi-resync';
      default:
        return 'Sync';
    }
  });

  readonly statusHeaderIcon = computed(() => {
    const label = this.detailedStatusLabel();
    switch (label) {
      case 'Transferring':
        return 'pi pi-spin pi-spinner text-sys-status-warning';
      case 'Checking':
        return 'pi pi-search text-sys-status-info';
      case 'Resolving':
      case 'Preparing':
        return 'pi pi-cog text-sys-status-info';
      case 'Completed':
        return 'pi pi-check-circle text-sys-status-success';
      case 'Error':
        return 'pi pi-times-circle text-sys-status-error';
      case 'Stopped':
        return 'pi pi-stop-circle text-sys-fg-muted';
      default:
        return 'pi pi-sync text-sys-status-info';
    }
  });

  readonly sortedTransfers = computed(() => {
    const s = this.syncStatus();
    if (!s?.transfers) return [];
    return [...s.transfers].sort((a, b) => {
      const priority = (status: string) => {
        if (status === 'transferring' || status === 'checking') return 0;
        if (status === 'failed') return 1;
        return 2;
      };
      return priority(a.status) - priority(b.status);
    });
  });

  readonly selectedTransfers = computed(() => {
    return this.fileGroups()[this.selectedFileTab()];
  });

  readonly fileGroups = computed(() => {
    const groups: Record<FileTabId, FileTransferInfo[]> = {
      syncing: [],
      check: [],
      complete: [],
      error: [],
    };

    for (const file of this.sortedTransfers()) {
      switch (file.status) {
        case 'transferring':
          groups.syncing.push(file);
          break;
        case 'checking':
        case 'checked':
          groups.check.push(file);
          break;
        case 'completed':
          groups.complete.push(file);
          break;
        case 'failed':
          groups.error.push(file);
          break;
      }
    }

    return groups;
  });

  readonly fileTabCounts = computed(() => {
    const groups = this.fileGroups();
    return {
      syncing: groups.syncing.length,
      check: groups.check.length,
      complete: groups.complete.length,
      error: groups.error.length,
    };
  });

  setFileTab(tab: FileTabId): void {
    this.selectedFileTab.set(tab);
  }

  selectNextFileTab(): void {
    this.moveFileTab(1);
  }

  selectPreviousFileTab(): void {
    this.moveFileTab(-1);
  }

  toggleExpanded(): void {
    this.isExpanded.update((expanded) => !expanded);
  }

  getFileTabButtonClass(tab: FileTabId): string {
    const selected = this.selectedFileTab() === tab;
    switch (tab) {
      case 'syncing':
        return selected ? 'bg-sys-status-warning-bg text-sys-status-warning shadow-neo-sm' : 'text-sys-status-warning hover:bg-sys-status-warning/10';
      case 'check':
        return selected ? 'bg-sys-status-info-bg text-sys-status-info shadow-neo-sm' : 'text-sys-status-info hover:bg-sys-status-info/10';
      case 'complete':
        return selected ? 'bg-sys-status-success-bg text-sys-status-success shadow-neo-sm' : 'text-sys-status-success hover:bg-sys-status-success/10';
      case 'error':
        return selected ? 'bg-sys-status-error-bg text-sys-status-error shadow-neo-sm' : 'text-sys-status-error hover:bg-sys-status-error/10';
    }
  }

  getFileRowBackground(file: FileTransferInfo): string | null {
    if (this.selectedFileTab() !== 'syncing') return null;
    const progress = Math.max(0, Math.min(100, file.progress || 0));
    return `linear-gradient(90deg, color-mix(in srgb, var(--color-status-warning) 24%, transparent) ${progress}%, color-mix(in srgb, var(--color-status-warning) 10%, transparent) ${progress}%)`;
  }

  private moveFileTab(direction: 1 | -1): void {
    const currentIndex = this.fileTabs.findIndex((tab) => tab.id === this.selectedFileTab());
    const nextIndex = (currentIndex + direction + this.fileTabs.length) % this.fileTabs.length;
    this.selectedFileTab.set(this.fileTabs[nextIndex].id);
  }

  formatBytes(bytes: number): string {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
  }

  formatSpeed(bytesPerSec: number): string {
    if (bytesPerSec < 1024) return bytesPerSec.toFixed(0) + ' B/s';
    if (bytesPerSec < 1024 * 1024) return (bytesPerSec / 1024).toFixed(1) + ' KB/s';
    if (bytesPerSec < 1024 * 1024 * 1024) return (bytesPerSec / (1024 * 1024)).toFixed(1) + ' MB/s';
    return (bytesPerSec / (1024 * 1024 * 1024)).toFixed(1) + ' GB/s';
  }

  getFileName(path: string): string {
    const parts = path.split('/');
    return parts[parts.length - 1] || path;
  }

  private toTitleCase(value: string): string {
    return value.charAt(0).toUpperCase() + value.slice(1);
  }

  getFileStatusIcon(file: FileTransferInfo): string {
    switch (file.status) {
      case 'transferring':
        return 'pi pi-spin pi-spinner text-sys-status-warning';
      case 'completed':
        return 'pi pi-check-circle text-sys-status-success';
      case 'failed':
        return 'pi pi-times-circle text-sys-status-error';
      case 'checking':
        return 'pi pi-spin pi-spinner text-sys-status-info';
      case 'checked':
        return 'pi pi-check-circle text-sys-status-info';
      default:
        return 'pi pi-circle text-sys-fg-muted';
    }
  }

  showError(error: string): void {
    this.selectedError = error;
    this.showErrorDialog = true;
  }

  getFileRowClass(file: FileTransferInfo): string {
    switch (file.status) {
      case 'transferring':
        return 'bg-sys-status-warning/10';
      case 'failed':
        return 'bg-sys-status-error/10';
      default:
        return '';
    }
  }
}
