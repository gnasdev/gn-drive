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
import { ErrorService } from "../services/error.service";
import { BehaviorSubject, Subscription } from "rxjs";
import { FormsModule } from "@angular/forms";
import {
  LucideAngularModule,
  Users,
  Plus,
  FolderOpen,
  X,
  Trash2,
  Download,
  Upload,
} from "lucide-angular";
import { models } from "../../../wailsjs/go/models";
import {
  parseRemotePath,
  buildRemotePath,
  parsePathConfig,
  buildPath,
  isValidProfileIndex,
  isValidPathIndex,
  DEFAULT_BANDWIDTH_OPTIONS,
  DEFAULT_PARALLEL_OPTIONS,
} from "./profiles.types";

// No Material imports needed anymore

@Component({
  selector: "app-profiles",
  imports: [CommonModule, FormsModule, LucideAngularModule],
  templateUrl: "./profiles.component.html",
  styleUrl: "./profiles.component.css",
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ProfilesComponent implements OnInit, OnDestroy {
  Date = Date;
  private subscriptions = new Subscription();

  // Lucide Icons
  readonly UsersIcon = Users;
  readonly PlusIcon = Plus;
  readonly FolderOpenIcon = FolderOpen;
  readonly XIcon = X;
  readonly Trash2Icon = Trash2;
  readonly DownloadIcon = Download;
  readonly UploadIcon = Upload;

  saveBtnText$ = new BehaviorSubject<string>("Save ✓");

  // Configuration options
  readonly bandwidthOptions = DEFAULT_BANDWIDTH_OPTIONS;
  readonly parallelOptions = DEFAULT_PARALLEL_OPTIONS;

  constructor(
    public readonly appService: AppService,
    private readonly cdr: ChangeDetectorRef,
    private readonly navigationService: NavigationService,
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

  async addProfile(): Promise<void> {
    try {
      const newProfileIndex = await this.appService.addProfile();
      // Navigate to edit the newly created profile
      this.navigationService.navigateToProfileEdit(newProfileIndex);
    } catch (error) {
      console.error("Error creating profile:", error);
      this.errorService.handleApiError(error, "create_profile");
    }
  }

  editProfile(index: number): void {
    this.navigationService.navigateToProfileEdit(index);
  }

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

  saveConfigInfo(): void {
    this.appService.saveConfigInfo();
    this.saveBtnText$.next("Saved ~");
    setTimeout(() => this.saveBtnText$.next("Save ✓"), 1000);
    this.cdr.detectChanges();
  }

  async exportProfiles(): Promise<void> {
    try {
      await this.appService.exportProfiles();
      this.errorService.showSuccess("Profiles exported successfully!");
    } catch (error) {
      console.error("Error exporting profiles:", error);
      this.errorService.handleApiError(error, "export_profiles");
    }
  }

  async importProfiles(): Promise<void> {
    try {
      await this.appService.importProfiles();
      this.errorService.showSuccess("Profiles imported successfully!");
    } catch (error) {
      console.error("Error importing profiles:", error);
      this.errorService.handleApiError(error, "import_profiles");
    }
  }

  addIncludePath(profileIndex: number): void {
    if (
      !isValidProfileIndex(
        this.appService.configInfo$.value.profiles,
        profileIndex
      )
    ) {
      console.error("Invalid profile index:", profileIndex);
      return;
    }
    this.appService.addIncludePath(profileIndex);
    this.cdr.detectChanges();
  }

  removeIncludePath(profileIndex: number, idx: number): void {
    const profiles = this.appService.configInfo$.value.profiles;
    if (
      !isValidProfileIndex(profiles, profileIndex) ||
      !isValidPathIndex(profiles[profileIndex].included_paths, idx)
    ) {
      console.error("Invalid indices:", { profileIndex, pathIndex: idx });
      return;
    }
    this.appService.removeIncludePath(profileIndex, idx);
    this.cdr.detectChanges();
  }

  addExcludePath(profileIndex: number): void {
    if (
      !isValidProfileIndex(
        this.appService.configInfo$.value.profiles,
        profileIndex
      )
    ) {
      console.error("Invalid profile index:", profileIndex);
      return;
    }
    this.appService.addExcludePath(profileIndex);
    this.cdr.detectChanges();
  }

  removeExcludePath(profileIndex: number, idx: number): void {
    const profiles = this.appService.configInfo$.value.profiles;
    if (
      !isValidProfileIndex(profiles, profileIndex) ||
      !isValidPathIndex(profiles[profileIndex].excluded_paths, idx)
    ) {
      console.error("Invalid indices:", { profileIndex, pathIndex: idx });
      return;
    }
    this.appService.removeExcludePath(profileIndex, idx);
    this.cdr.detectChanges();
  }

  trackByFn(index: number): number {
    return index;
  }

  // Helper method to generate number ranges for selectors
  getNumberRange(start: number, end: number): number[] {
    return Array.from({ length: end - start + 1 }, (_, i) => start + i);
  }

  // Get profile description for expansion panel
  getProfileDescription(profile: models.Profile): string {
    const from = profile.from || "Not configured";
    const to = profile.to || "Not configured";
    return `${from} → ${to}`;
  }

  // From path helpers
  getFromRemote(profile: models.Profile): string {
    const parsed = parseRemotePath(profile.from || "");
    return parsed.remote;
  }

  getFromPath(profile: models.Profile): string {
    const parsed = parseRemotePath(profile.from || "");
    return parsed.path;
  }

  updateFromPath(profile: models.Profile, remote: string, path: string): void {
    profile.from = buildRemotePath(remote, path);
    this.cdr.detectChanges();
  }

  // To path helpers
  getToRemote(profile: models.Profile): string {
    const parsed = parseRemotePath(profile.to || "");
    return parsed.remote;
  }

  getToPath(profile: models.Profile): string {
    const parsed = parseRemotePath(profile.to || "");
    return parsed.path;
  }

  updateToPath(profile: models.Profile, remote: string, path: string): void {
    profile.to = buildRemotePath(remote, path);
    this.cdr.detectChanges();
  }

  // Include path helpers
  getIncludePathType(profile: models.Profile, index: number): string {
    if (!isValidPathIndex(profile.included_paths, index)) {
      return "folder";
    }
    const parsed = parsePathConfig(profile.included_paths[index]);
    return parsed.type;
  }

  getIncludePathValue(profile: models.Profile, index: number): string {
    if (!isValidPathIndex(profile.included_paths, index)) {
      return "";
    }
    const parsed = parsePathConfig(profile.included_paths[index]);
    return parsed.value;
  }

  updateIncludePathType(
    profile: models.Profile,
    index: number,
    type: string
  ): void {
    if (!isValidPathIndex(profile.included_paths, index)) {
      console.error("Invalid path index:", index);
      return;
    }
    const currentValue = this.getIncludePathValue(profile, index);
    const pathConfig = { type: type as "file" | "folder", value: currentValue };
    profile.included_paths[index] = buildPath(pathConfig);
    this.cdr.detectChanges();
  }

  updateIncludePathValue(
    profile: models.Profile,
    index: number,
    value: string
  ): void {
    if (!isValidPathIndex(profile.included_paths, index)) {
      console.error("Invalid path index:", index);
      return;
    }
    const currentType = this.getIncludePathType(profile, index);
    const pathConfig = { type: currentType as "file" | "folder", value };
    profile.included_paths[index] = buildPath(pathConfig);
    this.cdr.detectChanges();
  }

  // Exclude path helpers
  getExcludePathType(profile: models.Profile, index: number): string {
    if (!isValidPathIndex(profile.excluded_paths, index)) {
      return "folder";
    }
    const parsed = parsePathConfig(profile.excluded_paths[index]);
    return parsed.type;
  }

  getExcludePathValue(profile: models.Profile, index: number): string {
    if (!isValidPathIndex(profile.excluded_paths, index)) {
      return "";
    }
    const parsed = parsePathConfig(profile.excluded_paths[index]);
    return parsed.value;
  }

  updateExcludePathType(
    profile: models.Profile,
    index: number,
    type: string
  ): void {
    if (!isValidPathIndex(profile.excluded_paths, index)) {
      console.error("Invalid path index:", index);
      return;
    }
    const currentValue = this.getExcludePathValue(profile, index);
    const pathConfig = { type: type as "file" | "folder", value: currentValue };
    profile.excluded_paths[index] = buildPath(pathConfig);
    this.cdr.detectChanges();
  }

  updateExcludePathValue(
    profile: models.Profile,
    index: number,
    value: string
  ): void {
    if (!isValidPathIndex(profile.excluded_paths, index)) {
      console.error("Invalid path index:", index);
      return;
    }
    const currentType = this.getExcludePathType(profile, index);
    const pathConfig = { type: currentType as "file" | "folder", value };
    profile.excluded_paths[index] = buildPath(pathConfig);
    this.cdr.detectChanges();
  }
}
