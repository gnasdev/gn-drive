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
import { models } from "../../../wailsjs/go/models";
import {
  parseRemotePath,
  buildRemotePath,
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

  saveBtnText$ = new BehaviorSubject<string>("Save ✓");
  profileIndex = 0; // Will be set from navigation
  profile: models.Profile | null = null;

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

  // Event handlers for new Tailwind form elements
  onFromRemoteChange(event: Event): void {
    const target = event.target as HTMLSelectElement;
    this.updateFromPath(target.value, this.getFromPath());
  }

  onFromPathChange(event: Event): void {
    const target = event.target as HTMLInputElement;
    this.updateFromPath(this.getFromRemote(), target.value);
  }

  onToRemoteChange(event: Event): void {
    const target = event.target as HTMLSelectElement;
    this.updateToPath(target.value, this.getToPath());
  }

  onToPathChange(event: Event): void {
    const target = event.target as HTMLInputElement;
    this.updateToPath(this.getToRemote(), target.value);
  }
}
