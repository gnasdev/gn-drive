import {
  ChangeDetectionStrategy,
  ChangeDetectorRef,
  Component,
  Directive,
  ElementRef,
  EventEmitter,
  HostListener,
  inject,
  Input,
  Output,
} from "@angular/core";
import { CommonModule } from "@angular/common";
import { FormsModule } from "@angular/forms";
import { ListFiles } from "../../../../wailsjs/desktop/backend/services/operationservice.js";
import * as models from "../../../../wailsjs/desktop/backend/models/models.js";

/* eslint-disable @angular-eslint/directive-selector */
@Directive({
  selector: "[clickOutside]",
  standalone: true,
})
/* eslint-enable @angular-eslint/directive-selector */
export class ClickOutsideDirective {
  private readonly elementRef = inject(ElementRef);

  @Output() clickOutside = new EventEmitter<void>();

  @HostListener("document:click", ["$event.target"])
  onClick(target: EventTarget | null): void {
    if (!target) return;
    const clickedInside = this.elementRef.nativeElement.contains(target);
    if (!clickedInside) {
      this.clickOutside.emit();
    }
  }
}

@Component({
  selector: "app-path-browser",
  imports: [CommonModule, FormsModule, ClickOutsideDirective],
  templateUrl: "./path-browser.component.html",
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class PathBrowserComponent {
  @Input() remoteName = "";
  @Input() path = "";
  @Input() placeholder = "";
  @Input() filterMode: "folder" | "file" | "both" = "both";
  @Input() disabled = false;

  @Output() pathChange = new EventEmitter<string>();

  entries: models.FileEntry[] = [];
  isLoading = false;
  showDropdown = false;
  browsingPath = "";
  filterPrefix = "";
  errorMessage = "";

  private readonly cdr = inject(ChangeDetectorRef);

  private lastLoadedKey = "";
  private debounceTimer: ReturnType<typeof setTimeout> | null = null;

  get filteredEntries(): models.FileEntry[] {
    let result = this.entries;
    if (this.filterMode === "folder") {
      result = result.filter((e) => e.is_dir);
    } else if (this.filterMode === "file") {
      result = result.filter((e) => !e.is_dir);
    }
    if (this.filterPrefix) {
      const lower = this.filterPrefix.toLowerCase();
      result = result.filter((e) => e.name.toLowerCase().startsWith(lower));
    }
    return result;
  }

  onInputChange(value: string): void {
    this.path = value;
    this.pathChange.emit(this.path);

    if (this.debounceTimer) clearTimeout(this.debounceTimer);
    this.debounceTimer = setTimeout(() => this.browseFromInput(), 300);
  }

  private async browseFromInput(): Promise<void> {
    const { dir, prefix } = this.splitPath(this.path);
    this.filterPrefix = prefix;
    this.browsingPath = dir;

    this.showDropdown = true;
    await this.loadEntries();

    if (this.filteredEntries.length === 0 && !this.isLoading) {
      this.showDropdown = false;
      this.cdr.detectChanges();
    }
  }

  toggleDropdown(): void {
    if (this.disabled) return;
    if (this.showDropdown) {
      this.closeDropdown();
    } else {
      this.browsingPath = this.path || "";
      this.filterPrefix = "";
      this.openDropdown();
    }
  }

  closeDropdown(): void {
    this.showDropdown = false;
    this.cdr.detectChanges();
  }

  private openDropdown(): void {
    this.showDropdown = true;
    this.loadEntries();
  }

  async loadEntries(): Promise<void> {
    const cacheKey = `${this.remoteName}:${this.browsingPath}`;
    if (cacheKey === this.lastLoadedKey && this.entries.length > 0) {
      return;
    }

    this.isLoading = true;
    this.cdr.detectChanges();

    try {
      this.errorMessage = "";
      const remotePath = this.buildRemotePath(this.browsingPath);
      const result = await ListFiles(remotePath, false);
      this.entries = (result || []).sort((a, b) => {
        if (a.is_dir !== b.is_dir) return a.is_dir ? -1 : 1;
        return a.name.localeCompare(b.name);
      });
      this.lastLoadedKey = cacheKey;
    } catch (err) {
      this.entries = [];
      this.lastLoadedKey = "";
      this.errorMessage = err instanceof Error ? err.message : String(err);
    } finally {
      this.isLoading = false;
      this.cdr.detectChanges();
    }
  }

  selectEntry(entry: models.FileEntry): void {
    const base = this.browsingPath.replace(/\/+$/, "");
    const newPath = `${base}/${entry.name}`;

    if (entry.is_dir) {
      this.browsingPath = newPath;
      this.filterPrefix = "";
      this.path = newPath;
      this.pathChange.emit(this.path);
      this.loadEntries();
    } else {
      this.path = newPath;
      this.pathChange.emit(this.path);
      this.closeDropdown();
    }
  }

  goUp(): void {
    if (!this.browsingPath) return;

    const parts = this.browsingPath.replace(/\/+$/, "").split("/");
    parts.pop();
    this.browsingPath = parts.join("/");
    this.filterPrefix = "";
    this.path = this.browsingPath;
    this.pathChange.emit(this.path);
    this.loadEntries();
  }

  private splitPath(path: string): { dir: string; prefix: string } {
    if (!path) return { dir: "", prefix: "" };
    if (path.endsWith("/")) return { dir: path.replace(/\/+$/, ""), prefix: "" };
    const lastSlash = path.lastIndexOf("/");
    if (lastSlash < 0) return { dir: "", prefix: path };
    return { dir: path.substring(0, lastSlash), prefix: path.substring(lastSlash + 1) };
  }

  private buildRemotePath(path: string): string {
    if (this.remoteName) {
      return `${this.remoteName}:${path || ""}`;
    }
    return path || "/";
  }
}
