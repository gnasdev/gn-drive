import { CommonModule } from "@angular/common";
import {
  ChangeDetectionStrategy,
  ChangeDetectorRef,
  Component,
  OnDestroy,
  OnInit,
} from "@angular/core";
import { AppService } from "../app.service";
import { NavigationService } from "../navigation.service";
import { BehaviorSubject, Subscription } from "rxjs";
import { FormsModule } from "@angular/forms";
import * as models from "../../../wailsjs/desktop/backend/models/models.js";
import {
  parseRemotePath,
  buildRemotePath,
  parsePathConfig,
  buildPath,
  isValidPathIndex,
  DEFAULT_BANDWIDTH_OPTIONS,
  DEFAULT_PARALLEL_OPTIONS,
} from "./profiles.types";
import {
  LucideAngularModule,
  ArrowLeft,
  Edit,
  FolderOpen,
  Zap,
  Wifi,
  Save,
  Trash,
  Plus,
  Minus,
  File,
  Folder,
  Check,
  X,
} from "lucide-angular";

// No Material imports needed anymore

@Component({
  selector: "app-profile-edit",
  imports: [CommonModule, FormsModule, LucideAngularModule],
  templateUrl: "./profile-edit.component.html",
  styleUrl: "./profile-edit.component.css",
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ProfileEditComponent implements OnInit, OnDestroy {
  Date = Date;
  private subscriptions = new Subscription();

  // Lucide Icons
  readonly ArrowLeftIcon = ArrowLeft;
  readonly EditIcon = Edit;
  readonly FolderOpenIcon = FolderOpen;
  readonly ZapIcon = Zap;
  readonly WifiIcon = Wifi;
  readonly SaveIcon = Save;
  readonly TrashIcon = Trash;
  readonly PlusIcon = Plus;
  readonly MinusIcon = Minus;
  readonly FileIcon = File;
  readonly FolderIcon = Folder;
  readonly CheckIcon = Check;
  readonly XIcon = X;

  saveBtnText$ = new BehaviorSubject<string>("Save");
  profileIndex = 0;
  profile: models.Profile | null = null;
  activeTab: 'general' | 'filters' | 'performance' | 'advanced' = 'general';

  readonly editorTabs = [
    { id: 'general' as const, label: 'General' },
    { id: 'filters' as const, label: 'Filters' },
    { id: 'performance' as const, label: 'Performance' },
    { id: 'advanced' as const, label: 'Advanced' },
  ];

  // Configuration options
  readonly bandwidthOptions = DEFAULT_BANDWIDTH_OPTIONS;
  readonly parallelOptions = DEFAULT_PARALLEL_OPTIONS;

  constructor(
    public readonly appService: AppService,
    private readonly cdr: ChangeDetectorRef,
    private readonly navigationService: NavigationService
  ) {}

  ngOnInit(): void {
    // Listen to navigation state to get profile index
    this.subscriptions.add(
      this.navigationService.currentState$.subscribe((state) => {
        if (state.page === "profile-edit") {
          this.profileIndex = state.profileIndex;
        }
      })
    );

    this.subscriptions.add(
      this.appService.configInfo$.subscribe((config) => {
        if (config?.profiles && this.profileIndex < config.profiles.length) {
          this.profile = config.profiles[this.profileIndex];
        }
        this.cdr.detectChanges();
      })
    );
    this.appService.getConfigInfo();
  }

  ngOnDestroy(): void {
    this.subscriptions.unsubscribe();
  }

  saveProfile(): void {
    if (this.profile) {
      // Ensure the profile is updated in the service before saving
      this.appService.updateProfile(this.profileIndex, this.profile);
    }
    this.appService.saveConfigInfo();
    this.saveBtnText$.next("Saved ~");
    setTimeout(() => this.saveBtnText$.next("Save ✓"), 1000);
    this.cdr.detectChanges();
  }

  // Method to handle profile field changes
  onProfileFieldChange(): void {
    if (this.profile) {
      this.appService.updateProfile(this.profileIndex, this.profile);
    }
  }

  goBack(): void {
    this.navigationService.navigateToProfiles();
  }

  // Helper method to generate number ranges for selectors
  getNumberRange(start: number, end: number): number[] {
    return Array.from({ length: end - start + 1 }, (_, i) => start + i);
  }

  // From path helpers
  getFromRemote(): string {
    if (!this.profile) return "";
    const parsed = parseRemotePath(this.profile.from || "");
    return parsed.remote;
  }

  getFromPath(): string {
    if (!this.profile) return "";
    const parsed = parseRemotePath(this.profile.from || "");
    return parsed.path;
  }

  updateFromPath(remote: string, path: string): void {
    if (!this.profile) return;
    this.profile.from = buildRemotePath(remote, path);
    this.appService.updateProfile(this.profileIndex, this.profile);
    this.cdr.detectChanges();
  }

  // To path helpers
  getToRemote(): string {
    if (!this.profile) return "";
    const parsed = parseRemotePath(this.profile.to || "");
    return parsed.remote;
  }

  getToPath(): string {
    if (!this.profile) return "";
    const parsed = parseRemotePath(this.profile.to || "");
    return parsed.path;
  }

  updateToPath(remote: string, path: string): void {
    if (!this.profile) return;
    this.profile.to = buildRemotePath(remote, path);
    this.appService.updateProfile(this.profileIndex, this.profile);
    this.cdr.detectChanges();
  }

  // Event handlers for new Tailwind form elements - removed as we now use ngModel

  // Include path methods
  addIncludePath(): void {
    if (!this.profile) return;
    this.profile.included_paths = [...this.profile.included_paths, "/**"];
    this.appService.updateProfile(this.profileIndex, this.profile);
    this.cdr.detectChanges();
  }

  removeIncludePath(index: number): void {
    if (!this.profile || !isValidPathIndex(this.profile.included_paths, index))
      return;
    this.profile.included_paths = this.profile.included_paths.filter(
      (_, i) => i !== index
    );
    this.appService.updateProfile(this.profileIndex, this.profile);
    this.cdr.detectChanges();
  }

  getIncludePathType(index: number): string {
    if (
      !this.profile ||
      !isValidPathIndex(this.profile.included_paths, index)
    ) {
      return "folder";
    }
    const parsed = parsePathConfig(this.profile.included_paths[index]);
    return parsed.type;
  }

  getIncludePathValue(index: number): string {
    if (
      !this.profile ||
      !isValidPathIndex(this.profile.included_paths, index)
    ) {
      return "";
    }
    const parsed = parsePathConfig(this.profile.included_paths[index]);
    return parsed.value;
  }

  updateIncludePathType(index: number, type: string): void {
    if (!this.profile || !isValidPathIndex(this.profile.included_paths, index))
      return;
    const currentValue = this.getIncludePathValue(index);
    const pathConfig = { type: type as "file" | "folder", value: currentValue };
    this.profile.included_paths[index] = buildPath(pathConfig);
    this.appService.updateProfile(this.profileIndex, this.profile);
    this.cdr.detectChanges();
  }

  updateIncludePathValue(index: number, value: string): void {
    if (!this.profile || !isValidPathIndex(this.profile.included_paths, index))
      return;
    const currentType = this.getIncludePathType(index);
    const pathConfig = { type: currentType as "file" | "folder", value };
    this.profile.included_paths[index] = buildPath(pathConfig);
    this.appService.updateProfile(this.profileIndex, this.profile);
    this.cdr.detectChanges();
  }

  // Exclude path methods
  addExcludePath(): void {
    if (!this.profile) return;
    this.profile.excluded_paths = [...this.profile.excluded_paths, "/**"];
    this.appService.updateProfile(this.profileIndex, this.profile);
    this.cdr.detectChanges();
  }

  removeExcludePath(index: number): void {
    if (!this.profile || !isValidPathIndex(this.profile.excluded_paths, index))
      return;
    this.profile.excluded_paths = this.profile.excluded_paths.filter(
      (_, i) => i !== index
    );
    this.appService.updateProfile(this.profileIndex, this.profile);
    this.cdr.detectChanges();
  }

  getExcludePathType(index: number): string {
    if (
      !this.profile ||
      !isValidPathIndex(this.profile.excluded_paths, index)
    ) {
      return "folder";
    }
    const parsed = parsePathConfig(this.profile.excluded_paths[index]);
    return parsed.type;
  }

  getExcludePathValue(index: number): string {
    if (
      !this.profile ||
      !isValidPathIndex(this.profile.excluded_paths, index)
    ) {
      return "";
    }
    const parsed = parsePathConfig(this.profile.excluded_paths[index]);
    return parsed.value;
  }

  updateExcludePathType(index: number, type: string): void {
    if (!this.profile || !isValidPathIndex(this.profile.excluded_paths, index))
      return;
    const currentValue = this.getExcludePathValue(index);
    const pathConfig = { type: type as "file" | "folder", value: currentValue };
    this.profile.excluded_paths[index] = buildPath(pathConfig);
    this.appService.updateProfile(this.profileIndex, this.profile);
    this.cdr.detectChanges();
  }

  updateExcludePathValue(index: number, value: string): void {
    if (!this.profile || !isValidPathIndex(this.profile.excluded_paths, index))
      return;
    const currentType = this.getExcludePathType(index);
    const pathConfig = { type: currentType as "file" | "folder", value };
    this.profile.excluded_paths[index] = buildPath(pathConfig);
    this.appService.updateProfile(this.profileIndex, this.profile);
    this.cdr.detectChanges();
  }

  // Event handlers for include paths
  onIncludePathTypeChange(index: number, event: Event): void {
    const target = event.target as HTMLSelectElement;
    this.updateIncludePathType(index, target.value);
  }

  onIncludePathValueChange(index: number, event: Event): void {
    const target = event.target as HTMLInputElement;
    this.updateIncludePathValue(index, target.value);
  }

  // Event handlers for exclude paths
  onExcludePathTypeChange(index: number, event: Event): void {
    const target = event.target as HTMLSelectElement;
    this.updateExcludePathType(index, target.value);
  }

  onExcludePathValueChange(index: number, event: Event): void {
    const target = event.target as HTMLInputElement;
    this.updateExcludePathValue(index, target.value);
  }

  // Track by function for ngFor
  trackByIndex(index: number): number {
    return index;
  }
}
