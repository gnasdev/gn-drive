import { Component, computed, input } from '@angular/core';
import { CommonModule } from '@angular/common';
import { SyncStatus, FileTransferInfo } from '../../models/sync-status.interface';
import { NeoDialogComponent } from '../neo/neo-dialog.component';

@Component({
  selector: 'app-operation-logs-panel',
  standalone: true,
  imports: [CommonModule, NeoDialogComponent],
  template: `
    @if (syncStatus(); as ss) {
      <div class="border-t-2 border-sys-border bg-sys-bg-inverse p-4 space-y-4">
        <!-- Progress header -->
        <div class="flex items-center justify-between">
          <span class="font-bold text-base text-sys-fg-inverse capitalize">{{ ss.status }}</span>
          <span class="font-bold text-lg text-sys-fg-inverse">{{ ss.progress.toFixed(1) }}%</span>
        </div>

        <!-- Progress bar -->
        <div class="w-full h-3 bg-sys-bg-tertiary border-2 border-sys-border">
          <div
            class="h-full transition-all duration-300"
            [class]="progressBarClass()"
            [style.width.%]="ss.progress"
          ></div>
        </div>

        <!-- Speed / Elapsed / ETA -->
        <div class="grid grid-cols-3 gap-3 text-sm">
          <div class="flex items-center gap-2 text-sys-fg-inverse font-medium">
            <i class="pi pi-bolt text-sm"></i>
            <span>{{ ss.speed }}</span>
          </div>
          <div class="flex items-center gap-2 text-sys-fg-inverse font-medium">
            <i class="pi pi-clock text-sm"></i>
            <span>{{ ss.elapsed_time }}</span>
          </div>
          <div class="flex items-center gap-2 text-sys-fg-inverse font-medium">
            @if (ss.eta && ss.eta !== '--') {
              <i class="pi pi-stopwatch text-sm"></i>
              <span>ETA {{ ss.eta }}</span>
            }
          </div>
        </div>

        <!-- Transfer stats -->
        <div class="grid grid-cols-2 gap-3 text-sm text-sys-fg-inverse font-medium">
          <div class="flex items-center gap-2">
            <i class="pi pi-file text-sm text-sys-fg-inverse"></i>
            <span>
              Files: {{ ss.files_transferred }}@if (ss.total_files > 0) { / {{ ss.total_files }}}
            </span>
          </div>
          <div class="flex items-center gap-2">
            <i class="pi pi-database text-sm text-sys-fg-inverse"></i>
            <span>
              Data: {{ formatBytes(ss.bytes_transferred) }}@if (ss.total_bytes > 0) { / {{ formatBytes(ss.total_bytes) }}}
            </span>
          </div>
        </div>

        <!-- Activity stats (only when non-zero) -->
        @if (ss.checks > 0 || ss.total_checks > 0 || ss.deletes > 0 || ss.errors > 0) {
          <div class="flex gap-4 text-sm font-medium">
            @if (ss.checks > 0 || ss.total_checks > 0) {
              <span class="text-sys-status-info">
                <i class="pi pi-check-circle text-sm mr-1"></i>Checks: {{ ss.checks }}@if (ss.total_checks > 0) { / {{ ss.total_checks }}}
              </span>
            }
            @if (ss.deletes > 0) {
              <span class="text-sys-status-warning">
                <i class="pi pi-trash text-sm mr-1"></i>Deletes: {{ ss.deletes }}
              </span>
            }
            @if (ss.errors > 0) {
              <span class="text-sys-status-error">
                <i class="pi pi-exclamation-circle text-sm mr-1"></i>Errors: {{ ss.errors }}
              </span>
            }
          </div>
        }

        <!-- File Transfers List -->
        @if (sortedTransfers().length > 0) {
          <div class="border-t-2 border-sys-border pt-3">
            <div class="flex items-center gap-2 mb-2">
              <i class="pi pi-list text-sm text-sys-fg-inverse"></i>
              <span class="font-bold text-sm text-sys-fg-inverse">Files ({{ sortedTransfers().length }})</span>
            </div>
            <div class="max-h-48 overflow-auto space-y-1">
              @for (file of sortedTransfers(); track file.name + file.status) {
                <div class="flex items-center gap-2 px-2 py-1.5 text-sm rounded"
                     [class]="getFileRowClass(file)">
                  <i [class]="getFileStatusIcon(file)" class="text-sm w-4 flex-shrink-0"></i>
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
              }
            </div>
          </div>
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
