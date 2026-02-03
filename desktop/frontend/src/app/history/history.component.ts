import { CommonModule } from "@angular/common";
import { ChangeDetectionStrategy, ChangeDetectorRef, Component } from "@angular/core";
import {
  LucideAngularModule,
  History,
  Trash2,
  CheckCircle,
  XCircle,
  Clock,
  Filter,
} from "lucide-angular";

interface HistoryEntry {
  id: string;
  profile_name: string;
  action: string;
  status: string;
  start_time: string;
  end_time: string;
  duration: string;
  files_transferred: number;
  bytes_transferred: number;
  errors: number;
  error_message?: string;
}

@Component({
  selector: "app-history",
  standalone: true,
  imports: [CommonModule, LucideAngularModule],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="flex flex-col h-full">
      <!-- Header -->
      <div
        class="flex items-center justify-between p-4 border-b border-gray-700"
      >
        <div class="flex items-center gap-3">
          <lucide-icon
            [img]="HistoryIcon"
            class="w-5 h-5 text-primary-400"
          ></lucide-icon>
          <h1 class="text-lg font-semibold text-gray-100">History</h1>
          <span class="text-sm text-gray-500"
            >{{ entries.length }} entr{{
              entries.length !== 1 ? "ies" : "y"
            }}</span
          >
        </div>
        <div class="flex items-center gap-2">
          <button
            (click)="showFilters = !showFilters"
            class="btn-secondary flex items-center gap-1.5 text-sm"
            [class.!bg-primary-600/20]="showFilters"
          >
            <lucide-icon [img]="FilterIcon" class="w-4 h-4"></lucide-icon>
            Filter
          </button>
          @if (entries.length > 0) {
          <button
            (click)="clearHistory()"
            class="p-2 rounded hover:bg-gray-700 transition-colors text-gray-400 hover:text-red-400"
            title="Clear History"
          >
            <lucide-icon [img]="Trash2Icon" class="w-4 h-4"></lucide-icon>
          </button>
          }
        </div>
      </div>

      <!-- Filter bar -->
      @if (showFilters) {
      <div class="flex items-center gap-3 px-4 py-2 border-b border-gray-700 bg-gray-800/50">
        <select
          class="select-field !w-auto text-sm"
          (change)="filterStatus = $any($event.target).value"
        >
          <option value="">All Statuses</option>
          <option value="success">Success</option>
          <option value="failed">Failed</option>
          <option value="cancelled">Cancelled</option>
        </select>
        <select
          class="select-field !w-auto text-sm"
          (change)="filterAction = $any($event.target).value"
        >
          <option value="">All Actions</option>
          <option value="pull">Pull</option>
          <option value="push">Push</option>
          <option value="bi">Bi-Sync</option>
          <option value="copy">Copy</option>
          <option value="move">Move</option>
        </select>
      </div>
      }

      <!-- History list -->
      <div class="flex-1 overflow-auto p-4">
        @if (filteredEntries.length === 0) {
        <div
          class="flex flex-col items-center justify-center h-48 text-gray-500"
        >
          <lucide-icon
            [img]="HistoryIcon"
            class="w-12 h-12 mb-3 opacity-30"
          ></lucide-icon>
          <p class="text-sm">No history entries</p>
        </div>
        } @else { @for (entry of filteredEntries; track entry.id) {
        <div
          class="panel mb-2 cursor-pointer hover:border-gray-600 transition-colors"
          (click)="toggleExpand(entry.id)"
        >
          <div class="flex items-center justify-between">
            <div class="flex items-center gap-3 min-w-0">
              <lucide-icon
                [img]="entry.status === 'success' ? CheckCircleIcon : XCircleIcon"
                class="w-4 h-4 shrink-0"
                [class]="
                  entry.status === 'success'
                    ? 'text-green-400'
                    : 'text-red-400'
                "
              ></lucide-icon>
              <div class="min-w-0">
                <div class="flex items-center gap-2">
                  <span class="text-gray-200 font-medium text-sm">{{
                    entry.profile_name
                  }}</span>
                  <span
                    class="text-xs px-2 py-0.5 rounded bg-gray-700 text-gray-400"
                    >{{ entry.action }}</span
                  >
                </div>
                <div class="text-xs text-gray-500 mt-0.5">
                  {{ entry.start_time }}
                </div>
              </div>
            </div>
            <div class="flex items-center gap-4 text-xs text-gray-500 shrink-0">
              <div class="flex items-center gap-1">
                <lucide-icon
                  [img]="ClockIcon"
                  class="w-3 h-3"
                ></lucide-icon>
                {{ entry.duration }}
              </div>
              <span>{{ entry.files_transferred }} files</span>
              <span>{{ formatBytes(entry.bytes_transferred) }}</span>
              @if (entry.errors > 0) {
              <span class="text-red-400">{{ entry.errors }} errors</span>
              }
            </div>
          </div>

          @if (expandedId === entry.id && entry.error_message) {
          <div
            class="mt-3 pt-3 border-t border-gray-700 text-xs text-red-400 font-mono whitespace-pre-wrap"
          >
            {{ entry.error_message }}
          </div>
          }
        </div>
        } }
      </div>
    </div>
  `,
})
export class HistoryComponent {
  readonly HistoryIcon = History;
  readonly Trash2Icon = Trash2;
  readonly CheckCircleIcon = CheckCircle;
  readonly XCircleIcon = XCircle;
  readonly ClockIcon = Clock;
  readonly FilterIcon = Filter;

  entries: HistoryEntry[] = [];
  showFilters = false;
  filterStatus = "";
  filterAction = "";
  expandedId: string | null = null;

  constructor(private readonly cdr: ChangeDetectorRef) {}

  get filteredEntries(): HistoryEntry[] {
    return this.entries.filter((e) => {
      if (this.filterStatus && e.status !== this.filterStatus) return false;
      if (this.filterAction && e.action !== this.filterAction) return false;
      return true;
    });
  }

  toggleExpand(id: string) {
    this.expandedId = this.expandedId === id ? null : id;
    this.cdr.detectChanges();
  }

  clearHistory() {
    this.entries = [];
    this.cdr.detectChanges();
    // TODO: Call HistoryService.ClearHistory via Wails bindings
  }

  formatBytes(bytes: number): string {
    if (bytes === 0) return "0 B";
    const units = ["B", "KB", "MB", "GB", "TB"];
    const i = Math.floor(Math.log(bytes) / Math.log(1024));
    return (bytes / Math.pow(1024, i)).toFixed(1) + " " + units[i];
  }
}
