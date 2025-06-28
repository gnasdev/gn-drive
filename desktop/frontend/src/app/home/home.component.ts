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

// Material Design imports
import { MatTabsModule } from "@angular/material/tabs";
import { MatCardModule } from "@angular/material/card";
import { MatButtonModule } from "@angular/material/button";
import { MatIconModule } from "@angular/material/icon";
import { MatSelectModule } from "@angular/material/select";
import { MatFormFieldModule } from "@angular/material/form-field";
import { MatProgressBarModule } from "@angular/material/progress-bar";
import { MatChipsModule } from "@angular/material/chips";
import { MatTooltipModule } from "@angular/material/tooltip";
import { MatMenuModule } from "@angular/material/menu";
import { FormsModule } from "@angular/forms";

@Component({
  selector: "app-home",
  imports: [
    CommonModule,
    MatTabsModule,
    MatCardModule,
    MatButtonModule,
    MatIconModule,
    MatSelectModule,
    MatFormFieldModule,
    MatProgressBarModule,
    MatChipsModule,
    MatTooltipModule,
    MatMenuModule,
    FormsModule,
  ],
  templateUrl: "./home.component.html",
  styleUrl: "./home.component.scss",
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

  ngOnInit() {
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

  ngOnDestroy() {
    this.changeDetectorSub?.unsubscribe();
  }

  changeProfile(e: Event) {
    this.appService.configInfo$.value.selected_profile_index = parseInt(
      (e.target as HTMLSelectElement).value
    );
    this.appService.configInfo$.next(this.appService.configInfo$.value);
    this.appService.saveConfigInfo();
  }

  validateCurrentProfileIndex(configInfo: models.ConfigInfo) {
    return configInfo.profiles?.[configInfo.selected_profile_index];
  }

  pull() {
    if (!this.validateCurrentProfileIndex(this.appService.configInfo$.value))
      return;
    this.appService.pull(
      this.appService.configInfo$.value.profiles[
        this.appService.configInfo$.value.selected_profile_index
      ]
    );
  }

  push() {
    if (!this.validateCurrentProfileIndex(this.appService.configInfo$.value))
      return;
    this.appService.push(
      this.appService.configInfo$.value.profiles[
        this.appService.configInfo$.value.selected_profile_index
      ]
    );
  }

  bi() {
    if (!this.validateCurrentProfileIndex(this.appService.configInfo$.value))
      return;
    this.appService.bi(
      this.appService.configInfo$.value.profiles[
        this.appService.configInfo$.value.selected_profile_index
      ]
    );
  }

  biResync() {
    if (!this.validateCurrentProfileIndex(this.appService.configInfo$.value))
      return;
    this.appService.bi(
      this.appService.configInfo$.value.profiles[
        this.appService.configInfo$.value.selected_profile_index
      ],
      true
    );
  }

  stopCommand() {
    this.appService.stopCommand();
  }

  // Tab management methods
  createTab() {
    this.tabService.createTab();
  }

  deleteTab(tabId: string) {
    const tab = this.tabService.getTab(tabId);
    if (tab && tab.currentTaskId) {
      this.appService.stopCommandForTab(tabId);
    }
    this.tabService.deleteTab(tabId);
  }

  setActiveTab(tabId: string) {
    this.tabService.setActiveTab(tabId);
  }

  // Tab-specific sync methods
  pullTab(tabId: string) {
    const tab = this.tabService.getTab(tabId);
    if (!tab || tab.selectedProfileIndex === null) return;

    const profile =
      this.appService.configInfo$.value.profiles[tab.selectedProfileIndex];
    if (profile) {
      this.appService.pullWithTab(profile, tabId);
    }
  }

  pushTab(tabId: string) {
    const tab = this.tabService.getTab(tabId);
    if (!tab || tab.selectedProfileIndex === null) return;

    const profile =
      this.appService.configInfo$.value.profiles[tab.selectedProfileIndex];
    if (profile) {
      this.appService.pushWithTab(profile, tabId);
    }
  }

  biTab(tabId: string) {
    const tab = this.tabService.getTab(tabId);
    if (!tab || tab.selectedProfileIndex === null) return;

    const profile =
      this.appService.configInfo$.value.profiles[tab.selectedProfileIndex];
    if (profile) {
      this.appService.biWithTab(profile, tabId);
    }
  }

  biResyncTab(tabId: string) {
    const tab = this.tabService.getTab(tabId);
    if (!tab || tab.selectedProfileIndex === null) return;

    const profile =
      this.appService.configInfo$.value.profiles[tab.selectedProfileIndex];
    if (profile) {
      this.appService.biWithTab(profile, tabId, true);
    }
  }

  stopCommandTab(tabId: string) {
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

  changeProfileTab(selectedValue: any, tabId: string) {
    if (selectedValue === null || selectedValue === undefined) {
      this.tabService.updateTab(tabId, { selectedProfileIndex: null });
    } else {
      const selectedIndex = parseInt(selectedValue);
      if (isNaN(selectedIndex)) {
        this.tabService.updateTab(tabId, { selectedProfileIndex: null });
      } else {
        this.tabService.updateTab(tabId, {
          selectedProfileIndex: selectedIndex,
        });
      }
    }
  }

  validateTabProfileIndex(tab: Tab): boolean {
    if (tab.selectedProfileIndex === null) return false;
    const profiles = this.appService.configInfo$.value.profiles;
    return (
      tab.selectedProfileIndex >= 0 &&
      tab.selectedProfileIndex < profiles.length
    );
  }

  // Tab rename methods
  startRenameTab(tabId: string) {
    this.tabService.startRenameTab(tabId);
  }

  finishRenameTab(tabId: string, newName: string) {
    this.tabService.finishRenameTab(tabId, newName);
  }

  cancelRenameTab(tabId: string) {
    this.tabService.cancelRenameTab(tabId);
  }

  // New methods for Material Design interface
  getActiveTabIndex(): number {
    const activeTabId = this.tabService.activeTabIdValue;
    if (!activeTabId) return 0;
    return this.tabService.tabsValue.findIndex((tab) => tab.id === activeTabId);
  }

  onTabChange(index: number) {
    const tabs = this.tabService.tabsValue;
    if (index >= 0 && index < tabs.length) {
      this.tabService.setActiveTab(tabs[index].id);
    }
  }

  getActionColor(action: Action): "primary" | "accent" | "warn" {
    switch (action) {
      case Action.Pull:
        return "primary";
      case Action.Push:
        return "accent";
      case Action.Bi:
        return "primary";
      case Action.BiResync:
        return "warn";
      default:
        return "primary";
    }
  }

  getActionIcon(action: Action): string {
    switch (action) {
      case Action.Pull:
        return "download";
      case Action.Push:
        return "upload";
      case Action.Bi:
        return "sync";
      case Action.BiResync:
        return "refresh";
      default:
        return "play_arrow";
    }
  }

  getActionLabel(action: Action): string {
    switch (action) {
      case Action.Pull:
        return "Pulling";
      case Action.Push:
        return "Pushing";
      case Action.Bi:
        return "Syncing";
      case Action.BiResync:
        return "Resyncing";
      default:
        return "Running";
    }
  }

  clearTabOutput(tabId: string) {
    this.tabService.updateTab(tabId, { data: [] });
  }
}
