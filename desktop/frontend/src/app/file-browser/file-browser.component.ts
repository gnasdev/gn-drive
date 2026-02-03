import { CommonModule } from "@angular/common";
import { ChangeDetectionStrategy, ChangeDetectorRef, Component } from "@angular/core";
import { FormsModule } from "@angular/forms";
import { ButtonModule } from "primeng/button";
import { InputText } from "primeng/inputtext";
import { Toolbar } from "primeng/toolbar";

interface FileEntry {
  path: string;
  name: string;
  size: number;
  mod_time: string;
  is_dir: boolean;
  mime_type?: string;
}

@Component({
  selector: "app-file-browser",
  standalone: true,
  imports: [CommonModule, FormsModule, ButtonModule, InputText, Toolbar],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="flex flex-col h-full">
      <!-- Header -->
      <p-toolbar>
        <ng-template #start>
          <div class="flex items-center gap-3">
            <i class="pi pi-folder-open text-primary-400"></i>
            <h1 class="text-lg font-semibold text-gray-100">File Browser</h1>
          </div>
        </ng-template>
        <ng-template #end>
          <div class="flex gap-2">
            <p-button
              (click)="refresh()"
              icon="pi pi-sync"
              label="Refresh"
              severity="secondary"
              size="small"
            ></p-button>
            <p-button
              icon="pi pi-plus"
              label="New Folder"
              severity="secondary"
              size="small"
            ></p-button>
            <p-button
              icon="pi pi-trash"
              label="Delete"
              severity="secondary"
              size="small"
            ></p-button>
          </div>
        </ng-template>
      </p-toolbar>

      <!-- Path bar -->
      <div class="px-4 py-3 border-b border-gray-700">
        <div class="flex gap-2">
          <input
            type="text"
            pInputText
            [(ngModel)]="currentPath"
            placeholder="remote:path (e.g. gdrive:documents)"
            class="flex-1"
            (keyup.enter)="browse()"
          />
          <p-button (click)="browse()" icon="pi pi-sync" label="Browse"></p-button>
        </div>

        <!-- Breadcrumbs -->
        @if (breadcrumbs.length > 0) {
        <div class="flex items-center gap-1 mt-2 text-sm text-gray-400">
          @for (crumb of breadcrumbs; track crumb; let last = $last) {
          <button
            (click)="navigateToBreadcrumb(crumb)"
            class="hover:text-primary-400 transition-colors"
          >
            {{ crumb }}
          </button>
          @if (!last) {
          <i class="pi pi-chevron-right" style="font-size: 0.75rem"></i>
          } }
        </div>
        }
      </div>

      <!-- File list -->
      <div class="flex-1 overflow-auto">
        @if (loading) {
        <div class="flex items-center justify-center h-32 text-gray-500">
          <p>Loading...</p>
        </div>
        } @else if (entries.length === 0) {
        <div class="flex flex-col items-center justify-center flex-1 py-16 text-gray-500">
          <i class="pi pi-folder-open text-5xl mb-3 opacity-30"></i>
          <p class="text-sm">Enter a remote path above to browse files</p>
        </div>
        } @else {
        <table class="w-full text-sm">
          <thead>
            <tr class="text-gray-400 border-b border-gray-700">
              <th class="text-left px-4 py-2 font-medium">Name</th>
              <th class="text-right px-4 py-2 font-medium w-24">Size</th>
              <th class="text-right px-4 py-2 font-medium w-40">Modified</th>
            </tr>
          </thead>
          <tbody>
            @for (entry of entries; track entry.path) {
            <tr
              class="border-b border-gray-800 hover:bg-gray-800/50 cursor-pointer transition-colors"
              (click)="entry.is_dir ? openDir(entry) : null"
            >
              <td class="px-4 py-2 flex items-center gap-2">
                <i
                  [class]="entry.is_dir ? 'pi pi-folder text-yellow-400' : 'pi pi-file text-gray-400'"
                  style="font-size: 1rem"
                ></i>
                <span class="text-gray-200">{{ entry.name }}</span>
              </td>
              <td class="px-4 py-2 text-right text-gray-400">
                {{ entry.is_dir ? '--' : formatSize(entry.size) }}
              </td>
              <td class="px-4 py-2 text-right text-gray-400">
                {{ formatDate(entry.mod_time) }}
              </td>
            </tr>
            }
          </tbody>
        </table>
        }
      </div>
    </div>
  `,
})
export class FileBrowserComponent {
  currentPath = "";
  entries: FileEntry[] = [];
  breadcrumbs: string[] = [];
  loading = false;

  constructor(private readonly cdr: ChangeDetectorRef) {}

  browse() {
    if (!this.currentPath) return;
    this.updateBreadcrumbs();
    this.loading = true;
    this.entries = [];
    this.cdr.detectChanges();

    // TODO: Call OperationService.ListFiles via Wails bindings
    setTimeout(() => {
      this.loading = false;
      this.cdr.detectChanges();
    }, 500);
  }

  refresh() {
    this.browse();
  }

  openDir(entry: FileEntry) {
    this.currentPath = this.currentPath.replace(/\/$/, "") + "/" + entry.name;
    this.browse();
  }

  navigateToBreadcrumb(crumb: string) {
    const idx = this.breadcrumbs.indexOf(crumb);
    if (idx >= 0) {
      const parts = this.currentPath.split(":");
      const remote = parts[0];
      const pathParts = (parts[1] || "").split("/").filter(Boolean);
      this.currentPath = remote + ":" + pathParts.slice(0, idx).join("/");
      this.browse();
    }
  }

  formatSize(bytes: number): string {
    if (bytes === 0) return "0 B";
    const units = ["B", "KB", "MB", "GB", "TB"];
    const i = Math.floor(Math.log(bytes) / Math.log(1024));
    return (bytes / Math.pow(1024, i)).toFixed(1) + " " + units[i];
  }

  formatDate(dateStr: string): string {
    if (!dateStr) return "--";
    try {
      return new Date(dateStr).toLocaleDateString();
    } catch {
      return dateStr;
    }
  }

  private updateBreadcrumbs() {
    const parts = this.currentPath.split(":");
    const remote = parts[0];
    const pathParts = (parts[1] || "").split("/").filter(Boolean);
    this.breadcrumbs = [remote + ":", ...pathParts];
  }
}
