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
import { ListFiles } from "../../../../wailsjs/desktop/backend/services/operationservice.js";

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

interface PathSegment {
  name: string;
  fullPath: string;
}

@Component({
  selector: "app-path-browser",
  imports: [CommonModule, ClickOutsideDirective],
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

  activeSegmentIndex: number | null = null; // -1 for root, 0+ for segments
  dropdownEntries: { name: string; is_dir: boolean }[] = [];
  dropdownLoading = false;
  lastSegmentIsFile = false;

  private readonly cdr = inject(ChangeDetectorRef);

  get segments(): PathSegment[] {
    if (!this.path) return [];
    const parts = this.path.split("/").filter((p) => p);
    return parts.map((name, i) => ({
      name,
      fullPath: parts.slice(0, i + 1).join("/"),
    }));
  }

  isLastSegmentFile(index: number): boolean {
    return index === this.segments.length - 1 && this.lastSegmentIsFile;
  }

  async onSegmentClick(index: number): Promise<void> {
    if (this.disabled) return;

    // Toggle dropdown off if clicking same segment
    if (this.activeSegmentIndex === index) {
      this.activeSegmentIndex = null;
      this.cdr.detectChanges();
      return;
    }

    this.activeSegmentIndex = index;
    this.dropdownLoading = true;
    this.dropdownEntries = [];
    this.cdr.detectChanges();

    try {
      const segmentPath = index === -1 ? "" : this.segments[index].fullPath;
      const remotePath = this.remoteName
        ? `${this.remoteName}:${segmentPath}`
        : segmentPath || "/";
      const entries = await ListFiles(remotePath, false);

      let filtered = entries || [];
      if (this.filterMode === "folder") {
        filtered = filtered.filter((e) => e.is_dir);
      } else if (this.filterMode === "file") {
        filtered = filtered.filter((e) => !e.is_dir);
      }

      this.dropdownEntries = filtered.sort((a, b) => {
        if (a.is_dir !== b.is_dir) return a.is_dir ? -1 : 1;
        return a.name.localeCompare(b.name);
      });
    } catch (err) {
      this.dropdownEntries = [];
      console.error("Failed to load entries:", err);
    } finally {
      this.dropdownLoading = false;
      this.cdr.detectChanges();
    }
  }

  onEntrySelect(entry: { name: string; is_dir: boolean }): void {
    if (this.activeSegmentIndex === null) return;

    const basePath =
      this.activeSegmentIndex === -1
        ? ""
        : this.segments[this.activeSegmentIndex].fullPath;

    this.path = basePath ? `${basePath}/${entry.name}` : entry.name;
    this.lastSegmentIsFile = !entry.is_dir;
    this.activeSegmentIndex = null;
    this.pathChange.emit(this.path);
    this.cdr.detectChanges();
  }

  closeDropdown(): void {
    if (this.activeSegmentIndex !== null) {
      this.activeSegmentIndex = null;
      this.cdr.detectChanges();
    }
  }
}
