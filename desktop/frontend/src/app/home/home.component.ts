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
import { TabService, Tab } from "../tab.service";
import { models } from "../../../wailsjs/go/models";
import {
  isValidProfileIndex,
  getActionConfig,
  parseProfileSelection,
  validateTabProfileSelection,
} from "./home.types";
import { FormsModule } from "@angular/forms";
import { SyncStatusComponent } from "../components/sync-status/sync-status.component";
import {
  LucideAngularModule,
  Settings,
  Plus,
  Download,
  Upload,
  RefreshCw,
  RotateCcw,
  Edit,
  Trash2,
  Play,
  Square,
  Eraser,
  FolderOpen,
  FileText,
  Cloud,
  ChevronDown,
  Terminal,
  Archive,
  StopCircle,
  Clock,
} from "lucide-angular";

@Component({
  selector: "app-home",
  imports: [
    CommonModule,
    FormsModule,
    LucideAngularModule,
    SyncStatusComponent,
  ],
  templateUrl: "./home.component.html",
  styleUrl: "./home.component.css",
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class HomeComponent implements OnInit, OnDestroy {
  Action = Action;

  // Lucide Icons
  readonly SettingsIcon = Settings;
  readonly PlusIcon = Plus;
  readonly DownloadIcon = Download;
  readonly UploadIcon = Upload;
  readonly RefreshCwIcon = RefreshCw;
  readonly RotateCcwIcon = RotateCcw;
  readonly EditIcon = Edit;
  readonly Trash2Icon = Trash2;
  readonly PlayIcon = Play;
  readonly SquareIcon = Square;
  readonly EraserIcon = Eraser;
  readonly FolderOpenIcon = FolderOpen;
  readonly FileTextIcon = FileText;
  readonly CloudIcon = Cloud;
  readonly ChevronDownIcon = ChevronDown;
  readonly TerminalIcon = Terminal;
  readonly ArchiveIcon = Archive;
  readonly StopCircleIcon = StopCircle;
  readonly ClockIcon = Clock;

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
    private readonly cdr: ChangeDetectorRef
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
          next: (tabs) => {
            console.log("HomeComponent tabs updated:", tabs);
            this.cdr.detectChanges();
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
    // Force change detection to ensure UI updates
    this.cdr.detectChanges();
  }

  deleteTab(tabId: string): void {
    const tab = this.tabService.getTab(tabId);

    // Check if tab has an active operation
    if (tab && tab.currentTaskId && tab.currentAction) {
      const confirmed = confirm(
        `This operation is currently running (${tab.currentAction}). ` +
          "Deleting this tab will stop the operation. Are you sure you want to continue?"
      );

      if (!confirmed) {
        return;
      }

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
    this.cdr.detectChanges();
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
    this.cdr.detectChanges();
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
    this.cdr.detectChanges();
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
    this.cdr.detectChanges();
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
      this.cdr.detectChanges();
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
    const tab = this.tabService.getTab(tabId);
    if (!tab) return;

    this.renameDialogData = {
      tabId: tabId,
      newName: tab.name,
    };

    this.showRenameDialog = true;

    // Focus the input after dialog opens
    setTimeout(() => {
      const input = document.querySelector(
        'input[type="text"]'
      ) as HTMLInputElement;
      if (input) {
        input.focus();
        input.select();
      }
    }, 100);
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

  getActionIcon(action: Action): typeof this.DownloadIcon {
    switch (action) {
      case Action.Pull:
        return this.DownloadIcon;
      case Action.Push:
        return this.UploadIcon;
      case Action.Bi:
      case Action.BiResync:
        return this.RefreshCwIcon;
      default:
        return this.RefreshCwIcon;
    }
  }

  getActionIconPath(action: Action): string {
    switch (action) {
      case Action.Pull:
        return "M7 16a4 4 0 01-.88-7.903A5 5 0 1115.9 6L16 6a5 5 0 011 9.9M9 19l3 3m0 0l3-3m-3 3V10";
      case Action.Push:
        return "M7 16a4 4 0 01-.88-7.903A5 5 0 1115.9 6L16 6a5 5 0 011 9.9M15 13l-3-3m0 0l-3 3m3-3v12";
      case Action.Bi:
      case Action.BiResync:
        return "M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15";
      default:
        return "M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15";
    }
  }

  getActionLabel(action: Action): string {
    const config = getActionConfig(action);
    return config.label;
  }

  onProfileChange(event: Event, tabId: string | undefined): void {
    const target = event.target as HTMLSelectElement;
    const value = target.value === "null" ? null : +target.value;
    this.changeProfileTab(value, tabId);
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
}
