import { CommonModule } from "@angular/common";
import { ChangeDetectionStrategy, ChangeDetectorRef, Component } from "@angular/core";
import { FormsModule } from "@angular/forms";
import { Card } from "primeng/card";
import { Toolbar } from "primeng/toolbar";
import { Tag } from "primeng/tag";
import { ButtonModule } from "primeng/button";
import { Select } from "primeng/select";

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
  imports: [CommonModule, FormsModule, Card, Toolbar, Tag, ButtonModule, Select],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="flex flex-col h-full">
      <!-- Header -->
      <p-toolbar>
        <ng-template #start>
          <div class="flex items-center gap-3">
            <i class="pi pi-history text-primary-400 text-xl"></i>
            <h1 class="text-lg font-semibold text-gray-100">History</h1>
            <span class="text-sm text-gray-500"
              >{{ entries.length }} entr{{
                entries.length !== 1 ? "ies" : "y"
              }}</span
            >
          </div>
        </ng-template>
        <ng-template #end>
          <div class="flex items-center gap-2">
            <p-button
              (click)="showFilters = !showFilters"
              [outlined]="!showFilters"
              severity="secondary"
              size="small"
            >
              <i class="pi pi-filter mr-1.5"></i>
              Filter
            </p-button>
            @if (entries.length > 0) {
            <p-button
              (click)="clearHistory()"
              [text]="true"
              severity="danger"
              size="small"
              pTooltip="Clear History"
            >
              <i class="pi pi-trash"></i>
            </p-button>
            }
          </div>
        </ng-template>
      </p-toolbar>

      <!-- Filter bar -->
      @if (showFilters) {
      <div class="flex items-center gap-3 px-4 py-2 border-b border-gray-700 bg-gray-800/50">
        <p-select
          [options]="statusOptions"
          [(ngModel)]="filterStatus"
          optionLabel="label"
          optionValue="value"
          placeholder="All Statuses"
          [style]="{ 'min-width': '10rem' }"
        ></p-select>
        <p-select
          [options]="actionOptions"
          [(ngModel)]="filterAction"
          optionLabel="label"
          optionValue="value"
          placeholder="All Actions"
          [style]="{ 'min-width': '10rem' }"
        ></p-select>
      </div>
      }

      <!-- History list -->
      <div class="flex-1 overflow-auto p-4">
        @if (filteredEntries.length === 0) {
        <div
          class="flex flex-col items-center justify-center h-48 text-gray-500"
        >
          <i class="pi pi-history text-5xl mb-3 opacity-30"></i>
          <p class="text-sm">No history entries</p>
        </div>
        } @else { @for (entry of filteredEntries; track entry.id) {
        <p-card
          class="mb-2 block cursor-pointer"
          [style]="{ 'margin-bottom': '0.5rem' }"
          (click)="toggleExpand(entry.id)"
        >
          <div class="flex items-center justify-between">
            <div class="flex items-center gap-3 min-w-0">
              <i
                class="shrink-0"
                [ngClass]="{
                  'pi pi-check-circle text-green-400': entry.status === 'success',
                  'pi pi-times-circle text-red-400': entry.status !== 'success'
                }"
              ></i>
              <div class="min-w-0">
                <div class="flex items-center gap-2">
                  <span class="text-gray-200 font-medium text-sm">{{
                    entry.profile_name
                  }}</span>
                  <p-tag
                    [value]="entry.action"
                    severity="secondary"
                  ></p-tag>
                </div>
                <div class="text-xs text-gray-500 mt-0.5">
                  {{ entry.start_time }}
                </div>
              </div>
            </div>
            <div class="flex items-center gap-4 text-xs text-gray-500 shrink-0">
              <div class="flex items-center gap-1">
                <i class="pi pi-clock text-xs"></i>
                {{ entry.duration }}
              </div>
              <span>{{ entry.files_transferred }} files</span>
              <span>{{ formatBytes(entry.bytes_transferred) }}</span>
              @if (entry.errors > 0) {
              <p-tag
                [value]="entry.errors + ' errors'"
                severity="danger"
              ></p-tag>
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
        </p-card>
        } }
      </div>
    </div>
  `,
})
export class HistoryComponent {
  entries: HistoryEntry[] = [];
  showFilters = false;
  filterStatus = "";
  filterAction = "";
  expandedId: string | null = null;

  statusOptions = [
    { label: "All Statuses", value: "" },
    { label: "Success", value: "success" },
    { label: "Failed", value: "failed" },
    { label: "Cancelled", value: "cancelled" },
  ];

  actionOptions = [
    { label: "All Actions", value: "" },
    { label: "Pull", value: "pull" },
    { label: "Push", value: "push" },
    { label: "Bi-Sync", value: "bi" },
    { label: "Copy", value: "copy" },
    { label: "Move", value: "move" },
  ];

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
