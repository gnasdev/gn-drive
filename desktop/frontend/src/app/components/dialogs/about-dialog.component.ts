/** GN Drive note: Renders a modal dialog used by the GN Drive desktop interface. */
import { CommonModule, DatePipe } from '@angular/common';
import {
  ChangeDetectionStrategy,
  ChangeDetectorRef,
  Component,
  EventEmitter,
  inject,
  Input,
  OnDestroy,
  OnInit,
  Output,
} from '@angular/core';
import { MessageService } from 'primeng/api';
import { Browser, Events } from '@wailsio/runtime';
import { GetAppInfo } from '../../../../wailsjs/desktop/backend/app';
import { UpdateInfo, UpdateStatus } from '../../../../wailsjs/desktop/backend/services/models';
import {
  CheckForUpdates,
  DownloadLatestUpdate,
  GetUpdateStatus,
  InstallDownloadedUpdate,
} from '../../../../wailsjs/desktop/backend/services/updateservice';
import { NeoButtonComponent } from '../neo/neo-button.component';
import { NeoCardComponent } from '../neo/neo-card.component';
import { NeoDialogComponent } from '../neo/neo-dialog.component';

interface AppInfo {
  name: string;
  version: string;
  commit: string;
  description: string;
}

@Component({
  selector: 'app-about-dialog',
  standalone: true,
  imports: [CommonModule, DatePipe, NeoDialogComponent, NeoButtonComponent, NeoCardComponent],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <neo-dialog
      [visible]="visible"
      (visibleChange)="visibleChange.emit($event)"
      title="About"
      maxWidth="500px"
      maxHeight="80vh"
      [headerYellow]="true"
    >
      <div class="space-y-4 h-full overflow-auto hide-scrollbar">
        <!-- App Info -->
        <neo-card>
          <div class="flex items-center gap-3 mb-3">
            <i class="pi pi-cloud text-2xl"></i>
            <div>
              <h2 class="font-bold text-lg">{{ appInfo?.name || 'GN Drive' }}</h2>
              <p class="text-xs text-sys-fg-muted">{{ appInfo?.description }}</p>
            </div>
          </div>
          <div class="space-y-1 text-sm">
            <div class="flex justify-between">
              <span class="text-sys-fg-muted">Version</span>
              <span class="font-mono">v{{ appInfo?.version || 'dev' }}</span>
            </div>
            @if (appInfo?.commit && appInfo!.commit !== 'unknown') {
              <div class="flex justify-between">
                <span class="text-sys-fg-muted">Commit</span>
                <span class="font-mono text-xs">{{ appInfo!.commit.slice(0, 7) }}</span>
              </div>
            }
          </div>
        </neo-card>

        <!-- Updates -->
        <neo-card>
          <div class="flex items-center gap-2 mb-3">
            <i class="pi pi-refresh"></i>
            <h2 class="font-bold">Updates</h2>
          </div>
          <div class="space-y-3">
            <div class="flex items-start justify-between gap-4">
              <div class="min-w-0">
                <p class="text-sm font-medium">App Version</p>
                <p class="text-xs text-sys-fg-muted font-mono">
                  v{{ updateInfo?.current_version || updateStatus?.current_version || appInfo?.version || 'dev' }}
                </p>
                @if (updateInfo?.unsupported) {
                  <p class="text-xs text-sys-status-warning mt-1">{{ updateInfo!.reason }}</p>
                } @else if (updateInfo?.has_update) {
                  <p class="text-xs text-sys-status-success mt-1">
                    v{{ updateInfo!.latest_version }} available
                  </p>
                } @else if (updateInfo && !updateInfo.has_update && !updateInfo.unsupported) {
                  <p class="text-xs text-sys-fg-muted mt-1">Up to date</p>
                }
              </div>
              <div class="flex flex-wrap justify-end gap-2">
                <neo-button
                  variant="secondary"
                  size="sm"
                  [loading]="isCheckingUpdate"
                  [disabled]="isDownloadingUpdate || isInstallingUpdate"
                  (onClick)="checkForUpdates()"
                >
                  Check
                </neo-button>
                @if (updateInfo?.release_url) {
                  <neo-button variant="secondary" size="sm" (onClick)="openReleaseNotes()">
                    Notes
                  </neo-button>
                }
              </div>
            </div>

            @if (updateStatus?.phase === 'downloading') {
              <div class="space-y-1">
                <div class="h-2 border-2 border-sys-border bg-sys-bg-secondary">
                  <div
                    class="h-full bg-sys-accent-primary"
                    [style.width.%]="updateProgressPercent"
                  ></div>
                </div>
                <p class="text-xs text-sys-fg-muted">
                  {{ formatBytes(updateStatus!.downloaded_bytes) }} / {{ formatBytes(updateStatus!.total_bytes) }}
                </p>
              </div>
            }

            @if (updateStatus?.error) {
              <div class="p-3 bg-sys-accent-danger/20 border-2 border-sys-border text-sm">
                {{ updateStatus!.error }}
              </div>
            }

            @if (updateInfo?.has_update && !updateInfo?.unsupported) {
              <div class="flex items-center justify-between gap-4">
                <div class="min-w-0">
                  <p class="text-sm font-medium truncate">{{ updateInfo!.asset_name }}</p>
                  <p class="text-xs text-sys-fg-muted">
                    {{ formatBytes(updateInfo!.asset_size) }}
                    @if (updateInfo!.published_at) {
                      &middot; {{ updateInfo!.published_at | date:'mediumDate' }}
                    }
                  </p>
                </div>
                <div class="flex gap-2">
                  @if (updateStatus?.phase !== 'downloaded') {
                    <neo-button
                      size="sm"
                      [loading]="isDownloadingUpdate"
                      [disabled]="isCheckingUpdate || isInstallingUpdate"
                      (onClick)="downloadUpdate()"
                    >
                      Download
                    </neo-button>
                  } @else {
                    <neo-button
                      size="sm"
                      [loading]="isInstallingUpdate"
                      [disabled]="isCheckingUpdate || isDownloadingUpdate"
                      (onClick)="installUpdate()"
                    >
                      Install
                    </neo-button>
                  }
                </div>
              </div>
            }
          </div>
        </neo-card>

        <!-- Author -->
        <neo-card>
          <div class="flex items-center gap-2 mb-2">
            <i class="pi pi-user"></i>
            <h2 class="font-bold">Author</h2>
          </div>
          <div class="text-sm">
            <p class="font-medium">gnas.dev</p>
            <a
              href="https://gnas.dev"
              (click)="openExternalLink($event, 'https://gnas.dev')"
              target="_blank"
              rel="noopener noreferrer"
              class="block text-xs text-primary-400 hover:underline font-mono"
            >
              https://gnas.dev
            </a>
          </div>
        </neo-card>
      </div>
    </neo-dialog>
  `,
})
export class AboutDialogComponent implements OnInit, OnDestroy {
  @Input() visible = false;
  @Output() visibleChange = new EventEmitter<boolean>();

  private readonly cdr = inject(ChangeDetectorRef);
  private readonly messageService = inject(MessageService);
  private updateEventCleanup: (() => void) | undefined;

  appInfo: AppInfo | null = null;
  updateInfo: UpdateInfo | null = null;
  updateStatus: UpdateStatus | null = null;
  isCheckingUpdate = false;
  isDownloadingUpdate = false;
  isInstallingUpdate = false;

  async ngOnInit(): Promise<void> {
    this.listenForUpdateEvents();
    await this.loadUpdateStatus();
    try {
      this.appInfo = await GetAppInfo();
      this.cdr.markForCheck();
    } catch (err) {
      console.error('Failed to load app info:', err);
    }
  }

  ngOnDestroy(): void {
    this.updateEventCleanup?.();
  }

  openExternalLink(event: MouseEvent, url: string): void {
    event.preventDefault();
    Browser.OpenURL(url).catch((err) => {
      console.error('Failed to open external link:', err);
    });
  }

  private listenForUpdateEvents(): void {
    this.updateEventCleanup = Events.On('tofe', (event) => {
      const parsed = this.parseBackendEvent(event.data);
      if (parsed?.type !== 'update:status') {
        return;
      }
      this.updateStatus = UpdateStatus.createFrom(parsed.data);
      this.isDownloadingUpdate = this.updateStatus.phase === 'downloading';
      this.isInstallingUpdate = this.updateStatus.phase === 'installing';
      this.cdr.markForCheck();
    });
  }

  private parseBackendEvent(rawData: unknown): { type?: string; data?: unknown } | null {
    if (!rawData) return null;
    try {
      return typeof rawData === 'string' ? JSON.parse(rawData) : rawData as { type?: string; data?: unknown };
    } catch {
      return null;
    }
  }

  private async loadUpdateStatus(): Promise<void> {
    try {
      this.updateStatus = await GetUpdateStatus();
      this.cdr.markForCheck();
    } catch (err) {
      console.error('Failed to load update status:', err);
    }
  }

  get updateProgressPercent(): number {
    if (!this.updateStatus?.total_bytes) return 0;
    return Math.min(100, Math.round((this.updateStatus.downloaded_bytes / this.updateStatus.total_bytes) * 100));
  }

  async checkForUpdates(): Promise<void> {
    this.isCheckingUpdate = true;
    this.cdr.markForCheck();
    try {
      this.updateInfo = await CheckForUpdates();
      if (this.updateInfo.unsupported) {
        this.messageService.add({
          severity: 'warn',
          summary: 'Update Check',
          detail: this.updateInfo.reason || 'This build cannot be self-updated',
        });
      } else if (this.updateInfo.has_update) {
        this.messageService.add({
          severity: 'info',
          summary: 'Update Available',
          detail: `Version ${this.updateInfo.latest_version} is ready to download`,
        });
      } else {
        this.messageService.add({
          severity: 'success',
          summary: 'Up to Date',
          detail: 'You are running the latest version',
        });
      }
    } catch (err) {
      this.messageService.add({
        severity: 'error',
        summary: 'Update Check Failed',
        detail: this.extractErrorMessage(err),
      });
    } finally {
      this.isCheckingUpdate = false;
      this.cdr.markForCheck();
    }
  }

  async downloadUpdate(): Promise<void> {
    this.isDownloadingUpdate = true;
    this.cdr.markForCheck();
    try {
      this.updateStatus = await DownloadLatestUpdate();
      this.messageService.add({
        severity: 'success',
        summary: 'Update Downloaded',
        detail: 'Ready to install and restart',
      });
    } catch (err) {
      this.messageService.add({
        severity: 'error',
        summary: 'Download Failed',
        detail: this.extractErrorMessage(err),
      });
    } finally {
      this.isDownloadingUpdate = false;
      this.cdr.markForCheck();
    }
  }

  async installUpdate(): Promise<void> {
    this.isInstallingUpdate = true;
    this.cdr.markForCheck();
    try {
      await InstallDownloadedUpdate();
    } catch (err) {
      this.isInstallingUpdate = false;
      this.messageService.add({
        severity: 'error',
        summary: 'Install Failed',
        detail: this.extractErrorMessage(err),
      });
      this.cdr.markForCheck();
    }
  }

  openReleaseNotes(): void {
    if (!this.updateInfo?.release_url) return;
    Browser.OpenURL(this.updateInfo.release_url).catch((err) => {
      console.error('Failed to open release notes:', err);
    });
  }

  formatBytes(bytes: number | undefined): string {
    if (!bytes || bytes <= 0) return '0 B';
    const units = ['B', 'KB', 'MB', 'GB'];
    let size = bytes;
    let unitIndex = 0;
    while (size >= 1024 && unitIndex < units.length - 1) {
      size /= 1024;
      unitIndex++;
    }
    return `${size.toFixed(unitIndex === 0 ? 0 : 1)} ${units[unitIndex]}`;
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
