import { CommonModule } from "@angular/common";
import { ChangeDetectionStrategy, ChangeDetectorRef, Component } from "@angular/core";
import { FormsModule } from "@angular/forms";
import {
  LucideAngularModule,
  FolderTree,
  RefreshCw,
  FolderPlus,
  Trash2,
  ChevronRight,
  File,
  Folder,
} from "lucide-angular";

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
  imports: [CommonModule, FormsModule, LucideAngularModule],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="flex flex-col h-full">
      <!-- Header -->
      <div class="p-4 border-b border-gray-700">
        <div class="flex items-center gap-3 mb-3">
          <lucide-icon [img]="FolderTreeIcon" class="w-5 h-5 text-primary-400"></lucide-icon>
          <h1 class="text-lg font-semibold text-gray-100">File Browser</h1>
        </div>

        <!-- Remote selector + path -->
        <div class="flex gap-2">
          <input
            type="text"
            [(ngModel)]="currentPath"
            placeholder="remote:path (e.g. gdrive:documents)"
            class="input-field flex-1"
            (keyup.enter)="browse()"
          />
          <button (click)="browse()" class="btn-primary flex items-center gap-1.5">
            <lucide-icon [img]="RefreshCwIcon" class="w-4 h-4"></lucide-icon>
            Browse
          </button>
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
          <lucide-icon [img]="ChevronRightIcon" class="w-3 h-3"></lucide-icon>
          } }
        </div>
        }

        <!-- Toolbar -->
        <div class="flex gap-2 mt-2">
          <button
            (click)="refresh()"
            class="px-3 py-1.5 text-xs rounded bg-gray-700 hover:bg-gray-600 text-gray-300 flex items-center gap-1.5"
          >
            <lucide-icon [img]="RefreshCwIcon" class="w-3 h-3"></lucide-icon>
            Refresh
          </button>
          <button
            class="px-3 py-1.5 text-xs rounded bg-gray-700 hover:bg-gray-600 text-gray-300 flex items-center gap-1.5"
          >
            <lucide-icon [img]="FolderPlusIcon" class="w-3 h-3"></lucide-icon>
            New Folder
          </button>
          <button
            class="px-3 py-1.5 text-xs rounded bg-gray-700 hover:bg-gray-600 text-gray-300 flex items-center gap-1.5"
          >
            <lucide-icon [img]="Trash2Icon" class="w-3 h-3"></lucide-icon>
            Delete
          </button>
        </div>
      </div>

      <!-- File list -->
      <div class="flex-1 overflow-auto">
        @if (loading) {
        <div class="flex items-center justify-center h-32 text-gray-500">
          <p>Loading...</p>
        </div>
        } @else if (entries.length === 0) {
        <div class="flex flex-col items-center justify-center h-48 text-gray-500">
          <lucide-icon [img]="FolderTreeIcon" class="w-12 h-12 mb-3 opacity-30"></lucide-icon>
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
                <lucide-icon
                  [img]="entry.is_dir ? FolderIcon : FileIcon"
                  class="w-4 h-4"
                  [class]="entry.is_dir ? 'text-yellow-400' : 'text-gray-400'"
                ></lucide-icon>
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
  readonly FolderTreeIcon = FolderTree;
  readonly RefreshCwIcon = RefreshCw;
  readonly FolderPlusIcon = FolderPlus;
  readonly Trash2Icon = Trash2;
  readonly ChevronRightIcon = ChevronRight;
  readonly FileIcon = File;
  readonly FolderIcon = Folder;

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
