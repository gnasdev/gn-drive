import { CommonModule } from "@angular/common";
import { Component, OnDestroy, OnInit } from "@angular/core";
import { map, Subscription } from "rxjs";
import { Action, AppService } from "../app.service";
import { TabService, Tab } from "../tab.service";
import { models } from "../../../wailsjs/go/models";
import {
  isValidProfileIndex,
  getActionConfig,
  parseProfileSelection,
  validateTabProfileSelection,
} from "./home.types";

// Material Design imports
import { MatCardModule } from "@angular/material/card";
import { MatButtonModule } from "@angular/material/button";
import { MatIconModule } from "@angular/material/icon";
import { MatSelectModule } from "@angular/material/select";
import { MatFormFieldModule } from "@angular/material/form-field";
import { MatInputModule } from "@angular/material/input";
import { MatProgressBarModule } from "@angular/material/progress-bar";
import { MatChipsModule } from "@angular/material/chips";
import { MatTooltipModule } from "@angular/material/tooltip";
import { MatMenuModule } from "@angular/material/menu";
import { MatListModule } from "@angular/material/list";
import { MatToolbarModule } from "@angular/material/toolbar";
import { MatTabsModule } from "@angular/material/tabs";
import { FormsModule } from "@angular/forms";

@Component({
  selector: "app-home",
  imports: [
    CommonModule,
    MatCardModule,
    MatButtonModule,
    MatIconModule,
    MatSelectModule,
    MatFormFieldModule,
    MatInputModule,
    MatProgressBarModule,
    MatChipsModule,
    MatTooltipModule,
    MatMenuModule,
    MatListModule,
    MatToolbarModule,
    MatTabsModule,
    FormsModule,
  ],
  templateUrl: "./home.component.html",
  styleUrl: "./home.component.css",
  // changeDetection: ChangeDetectionStrategy.OnPush, // Temporarily disable OnPush
})
export class HomeComponent implements OnInit, OnDestroy {
  Action = Action;

  private subscriptions = new Subscription();
  currentTabIdForMenu = "";
  isCurrentProfileValid: models.Profile | undefined;

  constructor(
    public readonly appService: AppService,
    public readonly tabService: TabService
  ) {}

  ngOnInit(): void {
    console.log("HomeComponent ngOnInit called");
    console.log(
      "AppService configInfo$ current value:",
      this.appService.configInfo$.value
    );
    console.log("TabService tabs current value:", this.tabService.tabsValue);
    console.log(
      "TabService activeTabId current value:",
      this.tabService.activeTabIdValue
    );

    // Check if subscriptions is already initialized
    if (!this.subscriptions || this.subscriptions.closed) {
      console.log("HomeComponent creating new subscriptions");
      this.subscriptions = new Subscription();
    }

    try {
      // Subscribe to profile validation with error handling
      this.subscriptions.add(
        this.appService.configInfo$
          .pipe(
            map((configInfo) => {
              console.log(
                "HomeComponent configInfo$ pipe received:",
                configInfo
              );
              if (!configInfo) {
                console.warn(
                  "HomeComponent received null/undefined configInfo"
                );
                return undefined;
              }
              return this.validateCurrentProfileIndex(configInfo);
            })
          )
          .subscribe({
            next: (profile) => {
              console.log("HomeComponent profile validation result:", profile);
              this.isCurrentProfileValid = profile;
            },
            error: (error) => {
              console.error(
                "HomeComponent configInfo$ subscription error:",
                error
              );
              // Reset to safe state on error
              this.isCurrentProfileValid = undefined;
            },
          })
      );
    } catch (error) {
      console.error("HomeComponent ngOnInit error:", error);
      // Reset to safe state on error
      this.isCurrentProfileValid = undefined;
    }
  }

  ngOnDestroy(): void {
    console.log("HomeComponent ngOnDestroy called");
    this.subscriptions.unsubscribe();
  }

  changeProfile(e: Event): void {
    const target = e.target as HTMLSelectElement;
    const selectedIndex = parseProfileSelection(target.value);

    if (
      selectedIndex !== null &&
      isValidProfileIndex(this.appService.configInfo$.value, selectedIndex)
    ) {
      // Create a new ConfigInfo instance to avoid mutating the current state
      const currentConfig = this.appService.configInfo$.value;
      const updatedConfigInfo = new models.ConfigInfo();
      Object.assign(updatedConfigInfo, currentConfig);
      updatedConfigInfo.selected_profile_index = selectedIndex;

      this.appService.configInfo$.next(updatedConfigInfo);
      this.appService.saveConfigInfo();
    }
  }

  validateCurrentProfileIndex(
    configInfo: models.ConfigInfo
  ): models.Profile | undefined {
    console.log("validateCurrentProfileIndex called with:", {
      configInfo,
      profiles: configInfo?.profiles,
      selectedIndex: configInfo?.selected_profile_index,
      profilesLength: configInfo?.profiles?.length,
    });

    if (
      !configInfo ||
      !configInfo.profiles ||
      !Array.isArray(configInfo.profiles)
    ) {
      console.warn(
        "validateCurrentProfileIndex: invalid configInfo or profiles"
      );
      return undefined;
    }

    if (
      typeof configInfo.selected_profile_index !== "number" ||
      configInfo.selected_profile_index < 0 ||
      configInfo.selected_profile_index >= configInfo.profiles.length
    ) {
      console.warn(
        "validateCurrentProfileIndex: invalid selected_profile_index"
      );
      return undefined;
    }

    const result = configInfo.profiles[configInfo.selected_profile_index];
    console.log("validateCurrentProfileIndex result:", result);
    return result;
  }

  pull(): void {
    const profile = this.validateCurrentProfileIndex(
      this.appService.configInfo$.value
    );
    if (!profile) return;
    this.appService.pull(profile);
  }

  push(): void {
    const profile = this.validateCurrentProfileIndex(
      this.appService.configInfo$.value
    );
    if (!profile) return;
    this.appService.push(profile);
  }

  bi(): void {
    const profile = this.validateCurrentProfileIndex(
      this.appService.configInfo$.value
    );
    if (!profile) return;
    this.appService.bi(profile);
  }

  biResync(): void {
    const profile = this.validateCurrentProfileIndex(
      this.appService.configInfo$.value
    );
    if (!profile) return;
    this.appService.bi(profile, true);
  }

  stopCommand(): void {
    this.appService.stopCommand();
  }

  // Tab management methods
  createTab(): void {
    this.tabService.createTab();
  }

  deleteTab(tabId: string): void {
    const tab = this.tabService.getTab(tabId);
    if (tab && tab.currentTaskId) {
      this.appService.stopCommandForTab(tabId);
    }
    this.tabService.deleteTab(tabId);
  }

  setActiveTab(tabId: string): void {
    this.tabService.setActiveTab(tabId);
  }

  // Tab-specific sync methods
  pullTab(tabId: string): void {
    const tab = this.tabService.getTab(tabId);
    if (
      !tab ||
      !validateTabProfileSelection(
        this.appService.configInfo$.value,
        tab.selectedProfileIndex
      )
    ) {
      return;
    }

    const profile =
      this.appService.configInfo$.value.profiles[tab.selectedProfileIndex!];
    this.appService.pullWithTab(profile, tabId);
  }

  pushTab(tabId: string): void {
    const tab = this.tabService.getTab(tabId);
    if (
      !tab ||
      !validateTabProfileSelection(
        this.appService.configInfo$.value,
        tab.selectedProfileIndex
      )
    ) {
      return;
    }

    const profile =
      this.appService.configInfo$.value.profiles[tab.selectedProfileIndex!];
    this.appService.pushWithTab(profile, tabId);
  }

  biTab(tabId: string): void {
    const tab = this.tabService.getTab(tabId);
    if (
      !tab ||
      !validateTabProfileSelection(
        this.appService.configInfo$.value,
        tab.selectedProfileIndex
      )
    ) {
      return;
    }

    const profile =
      this.appService.configInfo$.value.profiles[tab.selectedProfileIndex!];
    this.appService.biWithTab(profile, tabId);
  }

  biResyncTab(tabId: string): void {
    const tab = this.tabService.getTab(tabId);
    if (
      !tab ||
      !validateTabProfileSelection(
        this.appService.configInfo$.value,
        tab.selectedProfileIndex
      )
    ) {
      return;
    }

    const profile =
      this.appService.configInfo$.value.profiles[tab.selectedProfileIndex!];
    this.appService.biWithTab(profile, tabId, true);
  }

  stopCommandTab(tabId: string): void {
    const tab = this.tabService.getTab(tabId);
    if (tab) {
      console.log(
        "Stopping command for tab:",
        tabId,
        "currentTaskId:",
        tab.currentTaskId,
        "currentAction:",
        tab.currentAction
      );

      // Set stopping state immediately
      this.tabService.updateTab(tabId, {
        isStopping: true,
        data: [...(tab.data || []), "Stopping command..."],
      });

      this.appService.stopCommandForTab(tabId);
    }
  }

  changeProfileTab(
    selectedValue: string | number | null,
    tabId: string | undefined
  ): void {
    if (!tabId) return;
    const selectedIndex = parseProfileSelection(selectedValue);
    this.tabService.updateTab(tabId, { selectedProfileIndex: selectedIndex });
  }

  validateTabProfileIndex(tab: Tab): boolean {
    return validateTabProfileSelection(
      this.appService.configInfo$.value,
      tab.selectedProfileIndex
    );
  }

  // Tab rename methods
  startRenameTab(tabId: string): void {
    this.tabService.startRenameTab(tabId);
  }

  finishRenameTab(tabId: string, newName: string): void {
    this.tabService.finishRenameTab(tabId, newName);
  }

  cancelRenameTab(tabId: string): void {
    this.tabService.cancelRenameTab(tabId);
  }

  // New methods for Material Design interface
  getActiveTabIndex(): number {
    const activeTabId = this.tabService.activeTabIdValue;
    if (!activeTabId) return 0;
    const index = this.tabService.tabsValue.findIndex(
      (tab) => tab.id === activeTabId
    );
    return index >= 0 ? index : 0; // Always return valid index
  }

  onTabChange(index: number): void {
    try {
      const tabs = this.tabService.tabsValue;
      if (index >= 0 && index < tabs.length && tabs[index]) {
        this.tabService.setActiveTab(tabs[index].id);
      }
    } catch (error) {
      console.error("Error in onTabChange:", error);
    }
  }

  getActionColor(action: Action): "primary" | "accent" | "warn" {
    const config = getActionConfig(action);
    return config.color;
  }

  getActionIcon(action: Action): string {
    const config = getActionConfig(action);
    return config.icon;
  }

  getActionLabel(action: Action): string {
    const config = getActionConfig(action);
    return config.label;
  }

  clearTabOutput(tabId: string): void {
    this.tabService.updateTab(tabId, { data: [] });
  }

  setCurrentTabForMenu(tabId: string): void {
    this.currentTabIdForMenu = tabId;
  }

  trackByTabId(index: number, tab: Tab): string {
    return tab?.id || index.toString();
  }
}
