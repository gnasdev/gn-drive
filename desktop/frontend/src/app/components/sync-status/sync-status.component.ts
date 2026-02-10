import { Component, computed, input, ChangeDetectionStrategy } from "@angular/core";
import { CommonModule } from "@angular/common";
import { Card } from "primeng/card";
import { ProgressBar } from "primeng/progressbar";
import { Tag } from "primeng/tag";
import { SyncStatus, FileTransferInfo } from "../../models/sync-status.interface";
import { NeoDialogComponent } from "../neo/neo-dialog.component";

@Component({
  selector: "app-sync-status",
  standalone: true,
  imports: [CommonModule, Card, ProgressBar, Tag, NeoDialogComponent],
  templateUrl: "./sync-status.component.html",
  styleUrl: "./sync-status.component.scss",
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class SyncStatusComponent {
  readonly syncStatus = input<SyncStatus | null>(null);
  readonly showTitle = input(true);

  selectedError: string | null = null;
  showErrorDialog = false;
  readonly statusIcon = computed(() => {
    const s = this.syncStatus();
    if (!s) return "pi pi-sync";
    switch (s.status) {
      case "running": return "pi pi-sync";
      case "completed": return "pi pi-check-circle";
      case "error": return "pi pi-times-circle";
      case "stopped": return "pi pi-stop-circle";
      default: return "pi pi-sync";
    }
  });

  readonly actionIcon = computed(() => {
    const s = this.syncStatus();
    if (!s) return "pi pi-sync";
    switch (s.action) {
      case "pull": return "pi pi-download";
      case "push": return "pi pi-upload";
      case "bi":
      case "bi-resync": return "pi pi-sync";
      default: return "pi pi-sync";
    }
  });

  readonly actionLabel = computed(() => {
    const s = this.syncStatus();
    if (!s) return "Sync";
    switch (s.action) {
      case "pull": return "Pull";
      case "push": return "Push";
      case "bi": return "Bi-Sync";
      case "bi-resync": return "Bi-Resync";
      default: return "Sync";
    }
  });

  readonly progressValue = computed(() => this.syncStatus()?.progress || 0);

  readonly hasTransferData = computed(() => {
    const s = this.syncStatus();
    return !!(s && (s.files_transferred > 0 || s.bytes_transferred > 0 || s.total_files > 0 || s.total_bytes > 0));
  });

  readonly hasActivityData = computed(() => {
    const s = this.syncStatus();
    return !!(s && (s.checks > 0 || s.total_checks > 0 || s.deletes > 0 || s.renames > 0 || s.errors > 0));
  });

  readonly activeTransfers = computed(() => {
    const s = this.syncStatus();
    if (!s?.transfers) return [];
    return s.transfers.filter(t => t.status === "transferring" || t.status === "checking");
  });

  formatBytes(bytes: number): string {
    if (bytes === 0) return "0 B";
    const k = 1024;
    const sizes = ["B", "KB", "MB", "GB", "TB"];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + " " + sizes[i];
  }

  formatSpeed(speed: number): string {
    if (!speed || speed <= 0) return "";
    if (speed < 1024) return `${speed.toFixed(0)} B/s`;
    if (speed < 1024 * 1024) return `${(speed / 1024).toFixed(1)} KB/s`;
    if (speed < 1024 * 1024 * 1024) return `${(speed / (1024 * 1024)).toFixed(1)} MB/s`;
    return `${(speed / (1024 * 1024 * 1024)).toFixed(1)} GB/s`;
  }

  getFileName(path: string): string {
    const parts = path.split('/');
    return parts[parts.length - 1] || path;
  }

  getStatusLabel(status: string): string {
    switch (status) {
      case "checking": return "Checking";
      case "checked": return "Checked";
      case "transferring": return "Transferring";
      default: return status;
    }
  }

  getStatusTagColor(status: string): "info" | "warn" | "success" | "secondary" {
    switch (status) {
      case "checking": return "info";
      case "checked": return "info";
      case "transferring": return "warn";
      default: return "secondary";
    }
  }

  showError(error: string): void {
    this.selectedError = error;
    this.showErrorDialog = true;
  }

}
