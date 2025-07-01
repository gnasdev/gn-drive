import { CommonModule } from "@angular/common";
import {
  ChangeDetectionStrategy,
  ChangeDetectorRef,
  Component,
  OnInit,
  OnDestroy,
} from "@angular/core";
import { Action, AppService } from "./app.service.js";
import { combineLatest, Subscription } from "rxjs";
import { HomeComponent } from "./home/home.component.js";
import { models } from "../../wailsjs/go/models.js";
import { ProfilesComponent } from "./profiles/profiles.component.js";
import { ProfileEditComponent } from "./profiles/profile-edit.component.js";
import { RemotesComponent } from "./remotes/remotes.component.js";
import { NavigationService } from "./navigation.service.js";
import { ThemeService } from "./theme.service.js";

// Material Design imports
import { MatToolbarModule } from "@angular/material/toolbar";
import { MatIconModule } from "@angular/material/icon";
import { MatButtonModule } from "@angular/material/button";
import { MatTooltipModule } from "@angular/material/tooltip";
import { MatTabsModule } from "@angular/material/tabs";

@Component({
  selector: "app-root",
  imports: [
    CommonModule,
    HomeComponent,
    ProfilesComponent,
    ProfileEditComponent,
    RemotesComponent,
    MatToolbarModule,
    MatIconModule,
    MatButtonModule,
    MatTooltipModule,
    MatTabsModule,
  ],
  templateUrl: "./app.component.html",
  styleUrl: "./app.component.css",
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class AppComponent implements OnInit, OnDestroy {
  Action = Action;

  private subscriptions = new Subscription();
  private isInitialized = false;

  constructor(
    public readonly appService: AppService,
    private readonly cdr: ChangeDetectorRef,
    public readonly navigationService: NavigationService,
    public readonly themeService: ThemeService
  ) {
    console.log("AppComponent constructor called");
    console.log(
      "AppComponent navigationService initial state:",
      this.navigationService.currentState
    );
  }

  get isDarkMode(): boolean {
    return this.themeService.isDarkMode;
  }

  ngOnInit() {
    console.log(
      "AppComponent ngOnInit called, isInitialized:",
      this.isInitialized
    );

    if (this.isInitialized) {
      console.warn("AppComponent already initialized, skipping");
      return;
    }

    this.isInitialized = true;

    // Combine observables for change detection - remove tab$ to avoid circular updates
    this.subscriptions.add(
      combineLatest([
        this.appService.currentAction$,
        this.navigationService.currentState$,
        this.themeService.isDarkMode$,
      ]).subscribe(() => {
        console.log("AppComponent combineLatest triggered");
        this.cdr.detectChanges();
      })
    );
  }

  ngOnDestroy() {
    console.log("AppComponent ngOnDestroy called");
    this.isInitialized = false;
    this.subscriptions.unsubscribe();
  }

  async pull(profile: models.Profile) {
    this.navigationService.navigateToHome();
    this.appService.pull(profile);
  }

  async push(profile: models.Profile) {
    this.navigationService.navigateToHome();
    this.appService.push(profile);
  }

  async bi(profile: models.Profile) {
    this.navigationService.navigateToHome();
    this.appService.bi(profile);
  }

  stopCommand() {
    this.appService.stopCommand();
  }

  openHome() {
    this.navigationService.navigateToHome();
  }

  openProfiles() {
    this.navigationService.navigateToProfiles();
  }

  openRemotes() {
    this.navigationService.navigateToRemotes();
  }

  getSelectedTabIndex(): number {
    const currentState = this.navigationService.currentState;
    switch (currentState.page) {
      case "home":
        return 0;
      case "profiles":
      case "profile-edit":
        return 1;
      case "remotes":
        return 2;
      default:
        return 0;
    }
  }

  onTabChange(index: number): void {
    console.log("AppComponent onTabChange called with index:", index);
    switch (index) {
      case 0:
        this.navigationService.navigateToHome();
        break;
      case 1:
        this.navigationService.navigateToProfiles();
        break;
      case 2:
        this.navigationService.navigateToRemotes();
        break;
    }
  }

  toggleDarkMode() {
    console.log("Toggle dark mode called, current state:", this.isDarkMode);
    this.themeService.toggleDarkMode();
    console.log("Dark mode toggled to:", this.isDarkMode);
  }
}
