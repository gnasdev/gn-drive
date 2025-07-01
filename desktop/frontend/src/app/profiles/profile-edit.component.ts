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
  isValidProfileIndex,
  DEFAULT_BANDWIDTH_OPTIONS,
  DEFAULT_PARALLEL_OPTIONS,
} from "./profiles.types";

// Material Design imports
import { MatCardModule } from "@angular/material/card";
import { MatButtonModule } from "@angular/material/button";
import { MatIconModule } from "@angular/material/icon";
import { MatFormFieldModule } from "@angular/material/form-field";
import { MatInputModule } from "@angular/material/input";
import { MatSelectModule } from "@angular/material/select";
import { MatExpansionModule } from "@angular/material/expansion";
import { MatChipsModule } from "@angular/material/chips";
import { MatTooltipModule } from "@angular/material/tooltip";
import { MatSnackBarModule } from "@angular/material/snack-bar";
import { MatDividerModule } from "@angular/material/divider";
import { MatListModule } from "@angular/material/list";
import { MatToolbarModule } from "@angular/material/toolbar";

@Component({
  selector: "app-profile-edit",
  imports: [
    CommonModule,
    FormsModule,
    MatCardModule,
    MatButtonModule,
    MatIconModule,
    MatFormFieldModule,
    MatInputModule,
    MatSelectModule,
    MatExpansionModule,
    MatChipsModule,
    MatTooltipModule,
    MatSnackBarModule,
    MatDividerModule,
    MatListModule,
    MatToolbarModule,
  ],
  templateUrl: "./profile-edit.component.html",
  styleUrl: "./profile-edit.component.css",
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ProfileEditComponent implements OnInit, OnDestroy {
  Date = Date;
  private subscriptions = new Subscription();

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
    this.appService.saveConfigInfo();
    this.saveBtnText$.next("Saved ~");
    setTimeout(() => this.saveBtnText$.next("Save ✓"), 1000);
    this.cdr.detectChanges();
  }

  deleteProfile(): void {
    if (
      !isValidProfileIndex(
        this.appService.configInfo$.value.profiles,
        this.profileIndex
      )
    ) {
      console.error("Invalid profile index:", this.profileIndex);
      return;
    }
    this.appService.removeProfile(this.profileIndex);
    this.navigationService.navigateToProfiles();
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
    this.cdr.detectChanges();
  }
}
