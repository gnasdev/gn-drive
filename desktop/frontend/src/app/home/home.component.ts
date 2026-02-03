import { CommonModule } from "@angular/common";
import {
  Component,
  OnDestroy,
  OnInit,
  ChangeDetectorRef,
  ChangeDetectionStrategy,
} from "@angular/core";
import { map, Subscription } from "rxjs";
import { Action, AppService } from "../app.service";
import { TabService, Tab, OperationType, SyncDirection } from "../tab.service";
import * as models from "../../../wailsjs/desktop/backend/models/models.js";
import {
  isValidProfileIndex,
  getActionConfig,
  parseProfileSelection,
  validateTabProfileSelection,
} from "./home.types";
import { FormsModule } from "@angular/forms";
import { SyncStatusComponent } from "../components/sync-status/sync-status.component";
import { ToggleSwitch } from "primeng/toggleswitch";
import { Dialog } from "primeng/dialog";
import { ConfirmationService } from "primeng/api";
import { ButtonModule } from "primeng/button";
import { Card } from "primeng/card";
import { InputText } from "primeng/inputtext";
import { Select } from "primeng/select";

@Component({
  selector: "app-home",
  imports: [
    CommonModule,
    FormsModule,
    SyncStatusComponent,
    ToggleSwitch,
    Dialog,
    ButtonModule,
    Card,
    InputText,
    Select,
  ],
  templateUrl: "./home.component.html",
  styleUrl: "./home.component.css",
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class HomeComponent implements OnInit, OnDestroy {
  Action = Action;

  readonly operationTypes: { value: OperationType; label: string }[] = [
    { value: 'sync', label: 'Sync' },
    { value: 'copy', label: 'Copy' },
    { value: 'move', label: 'Move' },
    { value: 'check', label: 'Check' },
    { value: 'dedupe', label: 'Dedupe' },
  ];

  readonly syncDirections: { value: SyncDirection; label: string; icon: string }[] = [
    { value: 'pull', label: 'Pull', icon: 'pi pi-download' },
    { value: 'push', label: 'Push', icon: 'pi pi-upload' },
    { value: 'bi', label: 'Bi-Sync', icon: 'pi pi-sync' },
    { value: 'bi-resync', label: 'Resync', icon: 'pi pi-replay' },
  ];

  private subscriptions = new Subscription();
  showRenameDialog = false;

  renameDialogData = {
    tabId: "",
    newName: "",
  };
  isCurrentProfileValid: models.Profile | undefined;

  constructor(
    public readonly appService: AppService,
    public readonly tabService: TabService,
    private readonly cdr: ChangeDetectorRef,
    private readonly confirmationService: ConfirmationService
  ) {}

  ngOnInit(): void {    // Check if subscriptions is already initialized
    if (!this.subscriptions || this.subscriptions.closed) {      this.subscriptions = new Subscription();
    }

    try {
      // Subscribe to profile validation with error handling
      this.subscriptions.add(
        this.appService.configInfo$
          .pipe(
            map((configInfo) => {              if (!configInfo) {                return undefined;
              }
              return this.validateCurrentProfileIndex(configInfo);
            })
          )
          .subscribe({
            next: (profile) => {              this.isCurrentProfileValid = profile;
              this.cdr.detectChanges();
            },
            error: (error) => {
              console.error(
                "HomeComponent configInfo$ subscription error:",
                error
              );
              // Reset to safe state on error
              this.isCurrentProfileValid = undefined;
              this.cdr.detectChanges();
            },
          })
      );

      // Subscribe to tab changes for console output updates
      this.subscriptions.add(
        this.tabService.tabs.subscribe({
          next: (tabs) => {            this.cdr.detectChanges();
          },
          error: (error) => {
            console.error("HomeComponent tabs subscription error:", error);
          },
        })
      );
    } catch (error) {
      console.error("HomeComponent ngOnInit error:", error);
      // Reset to safe state on error
      this.isCurrentProfileValid = undefined;
      this.cdr.detectChanges();
    }
  }

  ngOnDestroy(): void {    this.subscriptions.unsubscribe();
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
  ): models.Profile | undefined {    if (
      !configInfo ||
      !configInfo.profiles ||
      !Array.isArray(configInfo.profiles)
    ) {      return undefined;
    }

    if (
      typeof configInfo.selected_profile_index !== "number" ||
      configInfo.selected_profile_index < 0 ||
      configInfo.selected_profile_index >= configInfo.profiles.length
    ) {      return undefined;
    }

    const result = configInfo.profiles[configInfo.selected_profile_index];    return result;
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

    // Check if tab has an active operation
    if (tab && tab.currentTaskId && tab.currentAction) {
      this.confirmationService.confirm({
        message: `This operation is currently running (${tab.currentAction}). Deleting this tab will stop the operation.`,
        header: "Delete Tab",
        acceptButtonStyleClass:
          "bg-red-600 hover:bg-red-700 text-white font-medium py-2 px-4 rounded-lg transition-colors ml-2",
        rejectButtonStyleClass:
          "bg-gray-700 hover:bg-gray-600 text-gray-200 font-medium py-2 px-4 rounded-lg transition-colors",
        accept: () => {
          this.appService.stopCommandForTab(tabId);
          this.tabService.deleteTab(tabId);
        },
      });
      return;
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
    if (tab) {      // Set stopping state immediately
      this.tabService.updateTab(tabId, {
        isStopping: true,
        data: [...(tab.data || []), "Stopping command..."],
      });

      this.appService.stopCommandForTab(tabId);
    }
  }

  clearTabOutput(tabId: string): void {
    this.tabService.updateTab(tabId, { data: [] });
  }

  changeProfileTab(
    selectedValue: string | number | null,
    tabId: string | undefined
  ): void {
    if (!tabId) return;
    const selectedIndex = parseProfileSelection(selectedValue);    this.tabService.updateTab(tabId, { selectedProfileIndex: selectedIndex });
  }

  validateTabProfileIndex(tab: Tab): boolean {
    return validateTabProfileSelection(
      this.appService.configInfo$.value,
      tab.selectedProfileIndex
    );
  }

  // Tab rename methods
  startRenameTab(tabId: string): void {
    const tab = this.tabService.getTab(tabId);
    if (!tab) return;

    this.renameDialogData = {
      tabId: tabId,
      newName: tab.name,
    };

    this.showRenameDialog = true;
  }

  confirmRename(): void {
    if (this.renameDialogData.newName.trim()) {
      this.tabService.finishRenameTab(
        this.renameDialogData.tabId,
        this.renameDialogData.newName.trim()
      );
    }
    this.showRenameDialog = false;
  }

  cancelRename(): void {
    this.showRenameDialog = false;
  }

  finishRenameTab(tabId: string, newName: string): void {
    this.tabService.finishRenameTab(tabId, newName);
  }

  cancelRenameTab(tabId: string): void {
    this.tabService.cancelRenameTab(tabId);
  }

  getActiveTabIndex(): number {
    const activeTabId = this.tabService.activeTabIdValue;
    if (!activeTabId) return 0;
    const index = this.tabService.tabsValue.findIndex(
      (tab) => tab.id === activeTabId
    );
    const result = index >= 0 ? index : 0;    return result; // Always return valid index
  }

  onTabChange(index: number): void {
    try {
      const tabs = this.tabService.tabsValue;      if (index >= 0 && index < tabs.length && tabs[index]) {
        const selectedTab = tabs[index];        this.tabService.setActiveTab(selectedTab.id);

        // Force change detection and update select element
        this.cdr.detectChanges();

        // Explicitly update the select element value after a short delay
        setTimeout(() => {
          this.updateSelectElementValue(selectedTab);
        }, 0);
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
    switch (action) {
      case Action.Pull:
        return 'pi pi-download';
      case Action.Push:
        return 'pi pi-upload';
      case Action.Bi:
      case Action.BiResync:
        return 'pi pi-sync';
      default:
        return 'pi pi-play';
    }
  }

  getActionLabel(action: Action): string {
    const config = getActionConfig(action);
    return config.label;
  }

  onProfileChange(value: number | null, tabId: string | undefined): void {
    this.changeProfileTab(value, tabId);
  }

  getProfileOptions(): { label: string; value: number | null }[] {
    const profiles = this.appService.configInfo$.value?.profiles || [];
    return [
      { label: 'No profile selected', value: null },
      ...profiles.map((p, i) => ({ label: p.name, value: i })),
    ];
  }

  trackByTabId(index: number, tab: Tab): string {
    return tab?.id || index.toString();
  }

  getSelectedProfile(tab: Tab | undefined): models.Profile | null {
    if (
      !tab ||
      tab.selectedProfileIndex === null ||
      tab.selectedProfileIndex === undefined
    ) {
      return null;
    }
    const configInfo = this.appService.configInfo$.value;
    if (
      !configInfo ||
      !configInfo.profiles ||
      tab.selectedProfileIndex >= configInfo.profiles.length ||
      tab.selectedProfileIndex < 0
    ) {
      return null;
    }
    return configInfo.profiles[tab.selectedProfileIndex];
  }

  getTabProfileValue(tab: Tab | undefined): number | null {
    if (!tab) {      return null;
    }
    const value = tab.selectedProfileIndex ?? null;    return value;
  }

  setOperationType(tabId: string, type: OperationType): void {
    this.tabService.updateTab(tabId, { operationType: type });
  }

  setSyncDirection(tabId: string, direction: SyncDirection): void {
    this.tabService.updateTab(tabId, { syncDirection: direction });
  }

  toggleDryRun(tabId: string): void {
    const tab = this.tabService.getTab(tabId);
    if (tab) {
      this.tabService.updateTab(tabId, { dryRun: !tab.dryRun });
    }
  }

  startOperation(tab: Tab): void {
    if (!this.validateTabProfileIndex(tab)) return;

    const profile =
      this.appService.configInfo$.value.profiles[tab.selectedProfileIndex!];
    const opType = tab.operationType || 'sync';

    if (opType === 'sync') {
      const direction = tab.syncDirection || 'push';
      switch (direction) {
        case 'pull':
          this.appService.pullWithTab(profile, tab.id);
          break;
        case 'push':
          this.appService.pushWithTab(profile, tab.id);
          break;
        case 'bi':
          this.appService.biWithTab(profile, tab.id);
          break;
        case 'bi-resync':
          this.appService.biWithTab(profile, tab.id, true);
          break;
      }
    }
    // TODO: Wire Copy, Move, Check, Dedupe to OperationService bindings
  }

  getOperationLabel(tab: Tab): string {
    const opType = tab.operationType || 'sync';
    if (opType === 'sync') {
      const dir = tab.syncDirection || 'push';
      const labels: Record<SyncDirection, string> = {
        'pull': 'Pull',
        'push': 'Push',
        'bi': 'Bi-Sync',
        'bi-resync': 'Resync',
      };
      return labels[dir];
    }
    const labels: Record<OperationType, string> = {
      'sync': 'Sync',
      'copy': 'Copy',
      'move': 'Move',
      'check': 'Check',
      'dedupe': 'Dedupe',
    };
    return labels[opType];
  }

  private updateSelectElementValue(tab: Tab): void {
    try {
      const selectElement = document.getElementById(
        `profile-select-${tab.id}`
      ) as HTMLSelectElement;
      if (selectElement) {
        const expectedValue = tab.selectedProfileIndex?.toString() ?? "null";        if (selectElement.value !== expectedValue) {
          selectElement.value = expectedValue;        }
      } else {      }
    } catch (error) {
      console.error("Error updating select element value:", error);
    }
  }
}
