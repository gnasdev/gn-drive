import { Component, Input } from '@angular/core';
import { CommonModule } from '@angular/common';
import { SyncStatus, FileTransferInfo } from '../../models/sync-status.interface';

@Component({
  selector: 'app-operation-logs-panel',
  standalone: true,
  imports: [CommonModule],
  template: `
    @if (syncStatus) {
      <div class="border-t-2 border-sys-border bg-sys-bg-inverse p-4 space-y-4">
        <!-- Progress header -->
        <div class="flex items-center justify-between">
          <span class="font-bold text-base text-sys-fg-inverse capitalize">{{ syncStatus.status }}</span>
          <span class="font-bold text-lg text-sys-accent-secondary">{{ syncStatus.progress.toFixed(1) }}%</span>
        </div>

        <!-- Progress bar -->
        <div class="w-full h-3 bg-sys-bg-tertiary border-2 border-sys-border">
          <div
            class="h-full transition-all duration-300"
            [class]="getProgressBarClass()"
            [style.width.%]="syncStatus.progress"
          ></div>
        </div>

        <!-- Speed / Elapsed / ETA -->
        <div class="grid grid-cols-3 gap-3 text-sm">
          <div class="flex items-center gap-2 text-sys-accent-secondary font-medium">
            <i class="pi pi-bolt text-sm"></i>
            <span>{{ syncStatus.speed }}</span>
          </div>
          <div class="flex items-center gap-2 text-sys-fg-inverse font-medium">
            <i class="pi pi-clock text-sm"></i>
            <span>{{ syncStatus.elapsed_time }}</span>
          </div>
          <div class="flex items-center gap-2 text-sys-fg-inverse font-medium">
            @if (syncStatus.eta && syncStatus.eta !== '--') {
              <i class="pi pi-stopwatch text-sm"></i>
              <span>ETA {{ syncStatus.eta }}</span>
            }
          </div>
        </div>

        <!-- Transfer stats -->
        <div class="grid grid-cols-2 gap-3 text-sm text-sys-fg-inverse font-medium">
          <div class="flex items-center gap-2">
            <i class="pi pi-file text-sm text-sys-accent-secondary"></i>
            <span>
              Files: {{ syncStatus.files_transferred }}@if (syncStatus.total_files > 0) { / {{ syncStatus.total_files }}}
            </span>
          </div>
          <div class="flex items-center gap-2">
            <i class="pi pi-database text-sm text-sys-accent-secondary"></i>
            <span>
              Data: {{ formatBytes(syncStatus.bytes_transferred) }}@if (syncStatus.total_bytes > 0) { / {{ formatBytes(syncStatus.total_bytes) }}}
            </span>
          </div>
        </div>

        <!-- Activity stats (only when non-zero) -->
        @if (syncStatus.checks > 0 || syncStatus.deletes > 0 || syncStatus.errors > 0) {
          <div class="flex gap-4 text-sm font-medium">
            @if (syncStatus.checks > 0) {
              <span class="text-sys-status-info">
                <i class="pi pi-check-circle text-sm mr-1"></i>Checks: {{ syncStatus.checks }}
              </span>
            }
            @if (syncStatus.deletes > 0) {
              <span class="text-sys-status-warning">
                <i class="pi pi-trash text-sm mr-1"></i>Deletes: {{ syncStatus.deletes }}
              </span>
            }
            @if (syncStatus.errors > 0) {
              <span class="text-sys-status-error">
                <i class="pi pi-exclamation-circle text-sm mr-1"></i>Errors: {{ syncStatus.errors }}
              </span>
            }
          </div>
        }

        <!-- File Transfers List -->
        @if (syncStatus.transfers && syncStatus.transfers.length > 0) {
          <div class="border-t-2 border-sys-border pt-3">
            <div class="flex items-center gap-2 mb-2">
              <i class="pi pi-list text-sm text-sys-accent-secondary"></i>
              <span class="font-bold text-sm text-sys-fg-inverse">Files ({{ syncStatus.transfers.length }})</span>
            </div>
            <div class="max-h-48 overflow-auto space-y-1">
              @for (file of syncStatus.transfers; track file.name + file.status) {
                <div class="flex items-center gap-2 px-2 py-1.5 text-sm rounded"
                     [class]="getFileRowClass(file)">
                  <i [class]="getFileStatusIcon(file)" class="text-sm w-4 flex-shrink-0"></i>
                  <span class="flex-1 min-w-0 truncate font-medium text-sys-fg-inverse" [title]="file.name">
                    {{ getFileName(file.name) }}
                  </span>
                  @if (file.status === 'transferring') {
                    <span class="text-sys-accent-secondary font-bold flex-shrink-0">{{ file.progress.toFixed(0) }}%</span>
                    @if (file.speed) {
                      <span class="text-sys-fg-inverse text-xs flex-shrink-0">{{ formatSpeed(file.speed) }}</span>
                    }
                  } @else {
                    <span class="text-sys-fg-muted text-xs flex-shrink-0">{{ formatBytes(file.bytes || file.size) }}</span>
                  }
                  @if (file.error) {
                    <span class="text-sys-status-error text-xs truncate flex-shrink-0 max-w-32" [title]="file.error">{{ file.error }}</span>
                  }
                </div>
              }
            </div>
          </div>
        }
      </div>
    }
  `,
})
export class OperationLogsPanelComponent {
  @Input() syncStatus: SyncStatus | null = null;

  getProgressBarClass(): string {
    if (!this.syncStatus) return 'bg-sys-accent-secondary';
    switch (this.syncStatus.status) {
      case 'completed':
        return 'bg-sys-status-success';
      case 'error':
        return 'bg-sys-status-error';
      case 'stopped':
        return 'bg-sys-fg-muted';
      default:
        return 'bg-sys-accent-secondary';
    }
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

  getFileStatusIcon(file: FileTransferInfo): string {
    switch (file.status) {
      case 'transferring':
        return 'pi pi-spin pi-spinner text-sys-accent-secondary';
      case 'completed':
        return 'pi pi-check-circle text-sys-status-success';
      case 'failed':
        return 'pi pi-times-circle text-sys-status-error';
      case 'checking':
        return 'pi pi-search text-sys-status-info';
      default:
        return 'pi pi-circle text-sys-fg-muted';
    }
  }

  getFileRowClass(file: FileTransferInfo): string {
    switch (file.status) {
      case 'transferring':
        return 'bg-sys-accent-secondary/10';
      case 'failed':
        return 'bg-sys-status-error/10';
      default:
        return '';
    }
  }
}
