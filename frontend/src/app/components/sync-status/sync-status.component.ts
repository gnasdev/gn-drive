import { Component, Input, ChangeDetectionStrategy } from "@angular/core";
import { CommonModule } from "@angular/common";
import {
  LucideAngularModule,
  Download,
  Upload,
  RotateCw,
  CheckCircle,
  XCircle,
  StopCircle,
  FileText,
  HardDrive,
  Check,
  Trash2,
  AlertCircle,
  FolderOpen,
  Moon,
  Zap,
  Clock,
  Timer,
} from "lucide-angular";
import { SyncStatus } from "../../models/sync-status.interface";

@Component({
  selector: "app-sync-status",
  standalone: true,
  imports: [CommonModule, LucideAngularModule],
  templateUrl: "./sync-status.component.html",
  styleUrl: "./sync-status.component.scss",
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class SyncStatusComponent {
  @Input() syncStatus: SyncStatus | null = null;
  @Input() showTitle = true;

  // Lucide icons
  readonly DownloadIcon = Download;
  readonly UploadIcon = Upload;
  readonly RotateCwIcon = RotateCw;
  readonly CheckCircleIcon = CheckCircle;
  readonly XCircleIcon = XCircle;
  readonly StopCircleIcon = StopCircle;
  readonly FileTextIcon = FileText;
  readonly HardDriveIcon = HardDrive;
  readonly CheckIcon = Check;
  readonly Trash2Icon = Trash2;
  readonly AlertCircleIcon = AlertCircle;
  readonly FolderOpenIcon = FolderOpen;
  readonly MoonIcon = Moon;
  readonly ZapIcon = Zap;
  readonly ClockIcon = Clock;
  readonly TimerIcon = Timer;

  getStatusColor(): string {
    if (!this.syncStatus) return "primary";

    switch (this.syncStatus.status) {
      case "running":
        return "primary";
      case "completed":
        return "accent";
      case "error":
        return "warn";
      case "stopped":
        return "basic";
      default:
        return "primary";
    }
  }

  getStatusIcon() {
    if (!this.syncStatus) return this.RotateCwIcon;

    switch (this.syncStatus.status) {
      case "running":
        return this.RotateCwIcon;
      case "completed":
        return this.CheckCircleIcon;
      case "error":
        return this.XCircleIcon;
      case "stopped":
        return this.StopCircleIcon;
      default:
        return this.RotateCwIcon;
    }
  }

  getActionIcon() {
    if (!this.syncStatus) return this.RotateCwIcon;

    switch (this.syncStatus.action) {
      case "pull":
        return this.DownloadIcon;
      case "push":
        return this.UploadIcon;
      case "bi":
      case "bi-resync":
        return this.RotateCwIcon;
      default:
        return this.RotateCwIcon;
    }
  }

  getActionLabel(): string {
    if (!this.syncStatus) return "Sync";

    switch (this.syncStatus.action) {
      case "pull":
        return "Pull";
      case "push":
        return "Push";
      case "bi":
        return "Bi-Sync";
      case "bi-resync":
        return "Bi-Resync";
      default:
        return "Sync";
    }
  }

  formatBytes(bytes: number): string {
    if (bytes === 0) return "0 B";

    const k = 1024;
    const sizes = ["B", "KB", "MB", "GB", "TB"];
    const i = Math.floor(Math.log(bytes) / Math.log(k));

    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + " " + sizes[i];
  }

  getProgressValue(): number {
    return this.syncStatus?.progress || 0;
  }

  hasTransferData(): boolean {
    return !!(
      this.syncStatus &&
      (this.syncStatus.files_transferred > 0 ||
        this.syncStatus.bytes_transferred > 0 ||
        this.syncStatus.total_files > 0 ||
        this.syncStatus.total_bytes > 0)
    );
  }

  hasActivityData(): boolean {
    return !!(
      this.syncStatus &&
      (this.syncStatus.checks > 0 ||
        this.syncStatus.deletes > 0 ||
        this.syncStatus.renames > 0 ||
        this.syncStatus.errors > 0)
    );
  }
}
