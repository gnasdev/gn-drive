import { CommonModule } from "@angular/common";
import {
  ChangeDetectionStrategy,
  ChangeDetectorRef,
  Component,
  OnDestroy,
  OnInit,
} from "@angular/core";
import { AppService } from "../app.service";
import { ErrorService } from "../services/error.service";
import { BehaviorSubject, Subscription } from "rxjs";
import { FormsModule } from "@angular/forms";
import { Card } from "primeng/card";
import { Toolbar } from "primeng/toolbar";
import { ButtonModule } from "primeng/button";
import { Dialog } from "primeng/dialog";
import { ToggleSwitch } from "primeng/toggleswitch";
import { InputText } from "primeng/inputtext";
import { InputNumber } from "primeng/inputnumber";
import { Select } from "primeng/select";
import { PathBrowserComponent } from "../components/path-browser/path-browser.component";
import * as models from "../../../wailsjs/desktop/backend/models/models.js";
import {
  parseRemotePath,
  buildRemotePath,
  parsePathConfig,
  buildPath,
  isValidProfileIndex,
  isValidPathIndex,
} from "./profiles.types";

@Component({
  selector: "app-profiles",
  imports: [
    CommonModule,
    FormsModule,
    Card,
    Toolbar,
    ButtonModule,
    Dialog,
    ToggleSwitch,
    InputText,
    InputNumber,
    Select,
    PathBrowserComponent,
  ],
  templateUrl: "./profiles.component.html",
  styleUrl: "./profiles.component.css",
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ProfilesComponent implements OnInit, OnDestroy {
  private subscriptions = new Subscription();

  // Dialog state
  showProfileDialog = false;
  editingProfile: models.Profile | null = null;
  editingProfileIndex: number | null = null; // null = create mode
  activeTab: "general" | "filters" | "performance" | "advanced" = "general";
  isSaving$ = new BehaviorSubject<boolean>(false);
  validationErrors: Record<string, string> = {};

  readonly editorTabs = [
    { id: "general" as const, label: "General", icon: "pi pi-folder-open" },
    { id: "filters" as const, label: "Filters", icon: "pi pi-file" },
    { id: "performance" as const, label: "Performance", icon: "pi pi-bolt" },
    { id: "advanced" as const, label: "Advanced", icon: "pi pi-cog" },
  ];

  readonly pathTypeOptions = [
    { label: "File", value: "file" },
    { label: "Folder", value: "folder" },
  ];

  readonly conflictResolutionOptions = [
    { label: "Default (newer wins)", value: "" },
    { label: "Newer", value: "newer" },
    { label: "Older", value: "older" },
    { label: "Larger", value: "larger" },
    { label: "Smaller", value: "smaller" },
    { label: "Path1 (Source)", value: "path1" },
    { label: "Path2 (Destination)", value: "path2" },
  ];

  constructor(
    public readonly appService: AppService,
    private readonly cdr: ChangeDetectorRef,
    private readonly errorService: ErrorService
  ) {}

  ngOnInit(): void {
    this.subscriptions.add(
      this.appService.configInfo$.subscribe(() => this.cdr.detectChanges())
    );
    this.appService.getConfigInfo();
  }

  ngOnDestroy(): void {
    this.subscriptions.unsubscribe();
  }

  // --- Dialog open/close/save ---

  openCreateDialog(): void {
    const profile = new models.Profile();
    profile.name = "";
    profile.from = "";
    profile.to = "";
    profile.included_paths = [];
    profile.excluded_paths = [];
    profile.parallel = 16;
    profile.bandwidth = 5;
    profile.backup_path = "";
    profile.cache_path = "";

    this.editingProfile = profile;
    this.editingProfileIndex = null;
    this.activeTab = "general";
    this.validationErrors = {};
    this.showProfileDialog = true;
  }

  openEditDialog(index: number): void {
    const profiles = this.appService.configInfo$.value.profiles;
    if (!isValidProfileIndex(profiles, index)) return;

    // Deep clone profile
    const source = profiles[index];
    const profile = new models.Profile();
    Object.assign(profile, source);
    profile.included_paths = [...(source.included_paths || [])];
    profile.excluded_paths = [...(source.excluded_paths || [])];

    this.editingProfile = profile;
    this.editingProfileIndex = index;
    this.activeTab = "general";
    this.validationErrors = {};
    this.showProfileDialog = true;
  }

  closeProfileDialog(): void {
    this.showProfileDialog = false;
    this.editingProfile = null;
    this.editingProfileIndex = null;
    this.validationErrors = {};
  }

  async saveProfileDialog(): Promise<void> {
    if (!this.editingProfile || !this.validateProfile()) return;

    this.isSaving$.next(true);
    this.cdr.detectChanges();

    try {
      if (this.editingProfileIndex === null) {
        // Create mode: add to config
        const currentConfig = this.appService.configInfo$.value;
        const updatedConfig = new models.ConfigInfo();
        Object.assign(updatedConfig, currentConfig);
        updatedConfig.profiles = [...currentConfig.profiles, this.editingProfile];
        this.appService.configInfo$.next(updatedConfig);
      } else {
        // Edit mode: update existing
        this.appService.updateProfile(this.editingProfileIndex, this.editingProfile);
      }
      await this.appService.saveConfigInfo();
      this.errorService.showSuccess(
        this.editingProfileIndex === null
          ? `Profile "${this.editingProfile.name}" created successfully!`
          : `Profile "${this.editingProfile.name}" updated successfully!`
      );
      this.closeProfileDialog();
    } catch (error) {
      console.error("Error saving profile:", error);
      this.errorService.handleApiError(error, "save_profile");
    } finally {
      this.isSaving$.next(false);
      this.cdr.detectChanges();
    }
  }

  // --- Validation ---

  validateProfile(): boolean {
    if (!this.editingProfile) return false;
    this.validationErrors = {};

    if (!this.editingProfile.name?.trim()) {
      this.validationErrors["name"] = "Profile name is required";
    }
    if (!this.editingProfile.from?.trim()) {
      this.validationErrors["from"] = "Source path is required";
    }
    if (!this.editingProfile.to?.trim()) {
      this.validationErrors["to"] = "Destination path is required";
    }

    this.cdr.detectChanges();
    return Object.keys(this.validationErrors).length === 0;
  }

  clearValidationError(field: string): void {
    delete this.validationErrors[field];
  }

  // --- Profile list actions ---

  async removeProfile(idx: number): Promise<void> {
    if (!isValidProfileIndex(this.appService.configInfo$.value.profiles, idx)) {
      console.error("Invalid profile index:", idx);
      return;
    }
    try {
      await this.appService.removeProfile(idx);
    } catch (error) {
      console.error("Error removing profile:", error);
      this.errorService.handleApiError(error, "remove_profile");
    }
  }

  getProfileDescription(profile: models.Profile): string {
    const from = profile.from || "Not configured";
    const to = profile.to || "Not configured";
    return `${from} \u2192 ${to}`;
  }

  // --- Remote options ---

  getRemoteOptions(): { label: string; value: string }[] {
    const remotes = this.appService.remotes$.value || [];
    return [
      { label: "Local", value: "" },
      ...remotes.map((r) => ({ label: r.name || "(Untitled Remote)", value: r.name })),
    ];
  }

  // --- From path helpers ---

  getFromRemote(): string {
    if (!this.editingProfile) return "";
    return parseRemotePath(this.editingProfile.from || "").remote;
  }

  getFromPath(): string {
    if (!this.editingProfile) return "";
    return parseRemotePath(this.editingProfile.from || "").path;
  }

  updateFromPath(remote: string, path: string): void {
    if (!this.editingProfile) return;
    this.editingProfile.from = buildRemotePath(remote, path);
    this.clearValidationError("from");
    this.cdr.detectChanges();
  }

  // --- To path helpers ---

  getToRemote(): string {
    if (!this.editingProfile) return "";
    return parseRemotePath(this.editingProfile.to || "").remote;
  }

  getToPath(): string {
    if (!this.editingProfile) return "";
    return parseRemotePath(this.editingProfile.to || "").path;
  }

  updateToPath(remote: string, path: string): void {
    if (!this.editingProfile) return;
    this.editingProfile.to = buildRemotePath(remote, path);
    this.clearValidationError("to");
    this.cdr.detectChanges();
  }

  // --- Include path methods ---

  addIncludePath(): void {
    if (!this.editingProfile) return;
    this.editingProfile.included_paths = [...this.editingProfile.included_paths, "/**"];
    this.cdr.detectChanges();
  }

  removeIncludePath(index: number): void {
    if (!this.editingProfile || !isValidPathIndex(this.editingProfile.included_paths, index)) return;
    this.editingProfile.included_paths = this.editingProfile.included_paths.filter((_, i) => i !== index);
    this.cdr.detectChanges();
  }

  getIncludePathType(index: number): string {
    if (!this.editingProfile || !isValidPathIndex(this.editingProfile.included_paths, index)) return "folder";
    return parsePathConfig(this.editingProfile.included_paths[index]).type;
  }

  getIncludePathValue(index: number): string {
    if (!this.editingProfile || !isValidPathIndex(this.editingProfile.included_paths, index)) return "";
    return parsePathConfig(this.editingProfile.included_paths[index]).value;
  }

  onIncludePathTypeChange(index: number, value: string): void {
    if (!this.editingProfile || !isValidPathIndex(this.editingProfile.included_paths, index)) return;
    const currentValue = this.getIncludePathValue(index);
    this.editingProfile.included_paths[index] = buildPath({ type: value as "file" | "folder", value: currentValue });
    this.cdr.detectChanges();
  }

  onIncludePathValueChange(index: number, event: Event): void {
    if (!this.editingProfile || !isValidPathIndex(this.editingProfile.included_paths, index)) return;
    const target = event.target as HTMLInputElement;
    this.onIncludePathValueUpdate(index, target.value);
  }

  onIncludePathValueUpdate(index: number, value: string): void {
    if (!this.editingProfile || !isValidPathIndex(this.editingProfile.included_paths, index)) return;
    const currentType = this.getIncludePathType(index);
    this.editingProfile.included_paths[index] = buildPath({ type: currentType as "file" | "folder", value });
    this.cdr.detectChanges();
  }

  // --- Exclude path methods ---

  addExcludePath(): void {
    if (!this.editingProfile) return;
    this.editingProfile.excluded_paths = [...this.editingProfile.excluded_paths, "/**"];
    this.cdr.detectChanges();
  }

  removeExcludePath(index: number): void {
    if (!this.editingProfile || !isValidPathIndex(this.editingProfile.excluded_paths, index)) return;
    this.editingProfile.excluded_paths = this.editingProfile.excluded_paths.filter((_, i) => i !== index);
    this.cdr.detectChanges();
  }

  getExcludePathType(index: number): string {
    if (!this.editingProfile || !isValidPathIndex(this.editingProfile.excluded_paths, index)) return "folder";
    return parsePathConfig(this.editingProfile.excluded_paths[index]).type;
  }

  getExcludePathValue(index: number): string {
    if (!this.editingProfile || !isValidPathIndex(this.editingProfile.excluded_paths, index)) return "";
    return parsePathConfig(this.editingProfile.excluded_paths[index]).value;
  }

  onExcludePathTypeChange(index: number, value: string): void {
    if (!this.editingProfile || !isValidPathIndex(this.editingProfile.excluded_paths, index)) return;
    const currentValue = this.getExcludePathValue(index);
    this.editingProfile.excluded_paths[index] = buildPath({ type: value as "file" | "folder", value: currentValue });
    this.cdr.detectChanges();
  }

  onExcludePathValueChange(index: number, event: Event): void {
    if (!this.editingProfile || !isValidPathIndex(this.editingProfile.excluded_paths, index)) return;
    const target = event.target as HTMLInputElement;
    this.onExcludePathValueUpdate(index, target.value);
  }

  onExcludePathValueUpdate(index: number, value: string): void {
    if (!this.editingProfile || !isValidPathIndex(this.editingProfile.excluded_paths, index)) return;
    const currentType = this.getExcludePathType(index);
    this.editingProfile.excluded_paths[index] = buildPath({ type: currentType as "file" | "folder", value });
    this.cdr.detectChanges();
  }

  // --- Helpers ---

  getNumberRangeOptions(start: number, end: number): { label: string; value: number }[] {
    return Array.from({ length: end - start + 1 }, (_, i) => ({
      label: String(start + i),
      value: start + i,
    }));
  }

  get dialogHeader(): string {
    return this.editingProfileIndex === null ? "Create Profile" : "Edit Profile";
  }
}
