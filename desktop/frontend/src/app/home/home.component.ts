import { CommonModule } from "@angular/common";
import {
  ChangeDetectionStrategy,
  ChangeDetectorRef,
  Component,
  OnDestroy,
  OnInit,
} from "@angular/core";
import { combineLatest, map, Subscription } from "rxjs";
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
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class HomeComponent implements OnInit, OnDestroy {
  Action = Action;

  private changeDetectorSub: Subscription | undefined;

  readonly isCurrentProfileValid$ = this.appService.configInfo$.pipe(
    map((configInfo) => this.validateCurrentProfileIndex(configInfo))
  );

  constructor(
    public readonly appService: AppService,
    public readonly tabService: TabService,
    private readonly cdr: ChangeDetectorRef
  ) {}

  ngOnInit(): void {
    this.changeDetectorSub = combineLatest([
      this.appService.data$,
      this.appService.configInfo$,
      this.isCurrentProfileValid$,
      this.appService.currentAction$,
      this.appService.currentId$,
      this.tabService.tabs,
      this.tabService.activeTabId,
    ]).subscribe(() => this.cdr.detectChanges());
  }

  ngOnDestroy(): void {
    this.changeDetectorSub?.unsubscribe();
  }

  changeProfile(e: Event): void {
    const target = e.target as HTMLSelectElement;
    const selectedIndex = parseProfileSelection(target.value);

    if (
      selectedIndex !== null &&
      isValidProfileIndex(this.appService.configInfo$.value, selectedIndex)
    ) {
      this.appService.configInfo$.value.selected_profile_index = selectedIndex;
      this.appService.configInfo$.next(this.appService.configInfo$.value);
      this.appService.saveConfigInfo();
    }
  }

  validateCurrentProfileIndex(
    configInfo: models.ConfigInfo
  ): models.Profile | undefined {
    return configInfo.profiles?.[configInfo.selected_profile_index];
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

  changeProfileTab(selectedValue: string | number | null, tabId: string): void {
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
    return this.tabService.tabsValue.findIndex((tab) => tab.id === activeTabId);
  }

  onTabChange(index: number): void {
    const tabs = this.tabService.tabsValue;
    if (index >= 0 && index < tabs.length) {
      this.tabService.setActiveTab(tabs[index].id);
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
}
