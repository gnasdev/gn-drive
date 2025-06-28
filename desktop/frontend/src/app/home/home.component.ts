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

@Component({
  selector: "app-home",
  imports: [CommonModule],
  templateUrl: "./home.component.html",
  styleUrl: "./home.component.css",
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
    this.appService.stopCommandForTab(tabId);
  }

  changeProfileTab(event: any, tabId: string) {
    const selectedIndex = parseInt(event.target.value);
    if (isNaN(selectedIndex)) {
      this.tabService.updateTab(tabId, { selectedProfileIndex: null });
    } else {
      this.tabService.updateTab(tabId, { selectedProfileIndex: selectedIndex });
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
}
