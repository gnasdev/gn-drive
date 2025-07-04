import { Injectable, OnDestroy } from "@angular/core";
import { BehaviorSubject } from "rxjs";
// Import models from the new v3 bindings
import * as config from "../../wailsjs/github.com/rclone/rclone/fs/config/models.js";
import * as models from "../../wailsjs/desktop/backend/models/models.js";

// Import functions from the new v3 bindings
import {
  Sync,
  SyncWithTabId,
  StopCommand,
  GetConfigInfo,
  UpdateProfiles,
  GetRemotes,
  DeleteRemote,
  AddRemote,
  StopAddingRemote,
  ExportProfiles,
  ImportProfiles,
  ExportRemotes,
  ImportRemotes,
} from "../../wailsjs/desktop/backend/app";
import { Events } from "@wailsio/runtime";
import { TabService } from "./tab.service";
import { ErrorService } from "./services/error.service";
import {
  SyncStatus,
  SyncStatusEvent,
  DEFAULT_SYNC_STATUS,
  isValidSyncStatus,
  isValidSyncAction,
} from "./models/sync-status.interface";

interface CommandDTO {
  command: string;
  pid: number | undefined;
  task: string | undefined;
  error: string | undefined;
  tab_id: string | undefined;
}

export enum Action {
  Pull = "pull",
  Push = "push",
  Bi = "bi",
  BiResync = "bi-resync",
}

@Injectable({
  providedIn: "root",
})
export class AppService implements OnDestroy {
  readonly currentId$ = new BehaviorSubject<number>(0);
  readonly currentAction$ = new BehaviorSubject<Action | undefined>(undefined);
  readonly data$ = new BehaviorSubject<string[]>([]);
  readonly configInfo$: BehaviorSubject<models.ConfigInfo>;
  readonly remotes$ = new BehaviorSubject<config.Remote[]>([]);
  readonly syncStatus$ = new BehaviorSubject<SyncStatus | null>(null);

  private eventCleanup: (() => void) | undefined;

  constructor(
    private tabService: TabService,
    private errorService: ErrorService
  ) {
    console.log("AppService constructor called");
    const configInfo = new models.ConfigInfo();
    configInfo.profiles = [];
    this.configInfo$ = new BehaviorSubject<models.ConfigInfo>(configInfo);
    console.log("AppService initial configInfo:", configInfo);

    // Store cleanup function for event listener
    this.eventCleanup = Events.On("tofe", (event) => {
      const rawData = event.data;
      const data = JSON.parse(rawData as string) as CommandDTO;

      // If event has tab_id, route to TabService
      if (data.tab_id) {
        this.tabService.handleCommandEvent(data);
        return;
      }

      // Legacy handling for events without tab_id
      switch (data.command) {
        case "command_started":
          this.replaceData("Command started...");
          this.syncStatus$.next(null); // Reset sync status
          break;
        case "command_stoped":
          this.currentAction$.next(undefined);
          this.currentId$.next(0);
          this.syncStatus$.next(null); // Clear sync status
          break;
        case "command_output":
          this.replaceData(data.error || "");
          break;
        case "error": {
          // Create new array to avoid mutating current state
          const dataValue = [
            ...this.data$.value,
            data.error || "Unknown error",
          ];
          this.data$.next(dataValue);
          break;
        }
        case "sync_status":
          // Handle sync status updates
          this.handleSyncStatusUpdate(data as SyncStatusEvent);
          break;
      }
    });

    console.log("AppService calling getConfigInfo and getRemotes");
    this.getConfigInfo();
    this.getRemotes();
  }

  ngOnDestroy() {
    console.log("AppService ngOnDestroy called");
    // Cleanup event listener
    if (this.eventCleanup) {
      console.log("AppService cleaning up event listener");
      this.eventCleanup();
      this.eventCleanup = undefined;
    }
  }

  replaceData(str: string) {
    this.data$.next([str]);
  }

  private handleSyncStatusUpdate(statusEvent: SyncStatusEvent) {
    const currentStatus = this.syncStatus$.value || { ...DEFAULT_SYNC_STATUS };

    // Update the current status with new data
    const updatedStatus: SyncStatus = {
      ...currentStatus,
      ...statusEvent,
      // Ensure required fields have defaults
      status:
        statusEvent.status && isValidSyncStatus(statusEvent.status)
          ? statusEvent.status
          : currentStatus.status,
      progress: statusEvent.progress ?? currentStatus.progress,
      speed: statusEvent.speed || currentStatus.speed,
      eta: statusEvent.eta || currentStatus.eta,
      files_transferred:
        statusEvent.files_transferred ?? currentStatus.files_transferred,
      total_files: statusEvent.total_files ?? currentStatus.total_files,
      bytes_transferred:
        statusEvent.bytes_transferred ?? currentStatus.bytes_transferred,
      total_bytes: statusEvent.total_bytes ?? currentStatus.total_bytes,
      current_file: statusEvent.current_file || currentStatus.current_file,
      errors: statusEvent.errors ?? currentStatus.errors,
      checks: statusEvent.checks ?? currentStatus.checks,
      deletes: statusEvent.deletes ?? currentStatus.deletes,
      renames: statusEvent.renames ?? currentStatus.renames,
      elapsed_time: statusEvent.elapsed_time || currentStatus.elapsed_time,
      action:
        statusEvent.action && isValidSyncAction(statusEvent.action)
          ? statusEvent.action
          : currentStatus.action,
    };

    this.syncStatus$.next(updatedStatus);
  }

  async pull(profile: models.Profile) {
    if (this.currentAction$.value === Action.Pull) return;

    this.replaceData("Pulling...");
    this.currentId$.next(await Sync(Action.Pull, profile));
    if (this.currentId$.value) this.currentAction$.next(Action.Pull);
  }

  async pullWithTab(profile: models.Profile, tabId: string) {
    const tab = this.tabService.getTab(tabId);
    if (!tab || tab.currentAction === Action.Pull) return;

    this.tabService.updateTab(tabId, { data: ["Pulling..."] });
    const taskId = await SyncWithTabId(Action.Pull, profile, tabId);
    if (taskId) {
      this.tabService.updateTab(tabId, {
        currentAction: Action.Pull,
        currentTaskId: taskId,
      });
    }
  }

  async push(profile: models.Profile) {
    if (this.currentAction$.value === Action.Push) return;

    this.replaceData("Pushing...");
    this.currentId$.next(await Sync(Action.Push, profile));
    if (this.currentId$.value) this.currentAction$.next(Action.Push);
  }

  async pushWithTab(profile: models.Profile, tabId: string) {
    const tab = this.tabService.getTab(tabId);
    if (!tab || tab.currentAction === Action.Push) return;

    this.tabService.updateTab(tabId, { data: ["Pushing..."] });
    const taskId = await SyncWithTabId(Action.Push, profile, tabId);
    if (taskId) {
      this.tabService.updateTab(tabId, {
        currentAction: Action.Push,
        currentTaskId: taskId,
      });
    }
  }

  async bi(profile: models.Profile, resync = false) {
    if (this.currentAction$.value === Action.Bi) return;

    this.replaceData("Bi...");
    this.currentId$.next(
      await Sync(resync ? Action.BiResync : Action.Bi, profile)
    );
    if (this.currentId$.value) this.currentAction$.next(Action.Bi);
  }

  async biWithTab(profile: models.Profile, tabId: string, resync = false) {
    const tab = this.tabService.getTab(tabId);
    if (!tab || tab.currentAction === Action.Bi) return;

    this.tabService.updateTab(tabId, { data: ["Bi..."] });
    const taskId = await SyncWithTabId(
      resync ? Action.BiResync : Action.Bi,
      profile,
      tabId
    );
    if (taskId) {
      this.tabService.updateTab(tabId, {
        currentAction: Action.Bi,
        currentTaskId: taskId,
      });
    }
  }

  stopCommand() {
    if (!this.currentAction$.value) return;
    StopCommand(this.currentId$.value);
  }

  stopCommandForTab(tabId: string) {
    const tab = this.tabService.getTab(tabId);
    if (!tab || !tab.currentAction || !tab.currentTaskId) return;
    StopCommand(tab.currentTaskId);
  }

  async getConfigInfo() {
    console.log("AppService getConfigInfo called");
    try {
      const configInfo = await GetConfigInfo();
      console.log(
        "AppService getConfigInfo received from backend:",
        configInfo
      );
      configInfo.profiles = configInfo.profiles ?? [];
      console.log(
        "AppService getConfigInfo after profiles normalization:",
        configInfo
      );
      this.configInfo$.next(configInfo);
      console.log("AppService getConfigInfo emitted to configInfo$");
    } catch (e) {
      console.error("AppService getConfigInfo error:", e);
      this.errorService.handleApiError(e, "get_config_info");
    }
  }

  async getRemotes(): Promise<void> {
    try {
      const remotes = await GetRemotes();
      this.remotes$.next(remotes ?? []);
    } catch (error) {
      console.error("Error getting remotes:", error);
      this.errorService.handleApiError(error, "get_remotes");
      throw new Error("Failed to get remotes");
    }
  }

  async addRemote(objData: Record<string, string>): Promise<void> {
    if (!objData["name"] || !objData["type"]) {
      throw new Error("Remote name and type are required");
    }

    try {
      await AddRemote(objData["name"], objData["type"], {});
      await this.getRemotes();
    } catch (error) {
      console.error("Error adding remote:", error);
      this.errorService.handleApiError(error, "add_remote");
      throw error;
    }
  }

  async stopAddingRemote(): Promise<void> {
    try {
      await StopAddingRemote();
    } catch (error) {
      console.error("Error stopping add remote:", error);
      this.errorService.handleApiError(error, "stop_adding_remote");
      throw error;
    }
  }

  async deleteRemote(name: string): Promise<void> {
    if (!name) {
      throw new Error("Remote name is required");
    }

    try {
      await DeleteRemote(name);
      // Refresh both remotes and config info since profiles might have been deleted
      await this.getRemotes();
      await this.getConfigInfo();
    } catch (error) {
      console.error("Error deleting remote:", error);
      this.errorService.handleApiError(error, "delete_remote");
      throw new Error("Failed to delete remote");
    }
  }

  async addProfile(): Promise<number> {
    const profile = new models.Profile();
    profile.name = ""; // Start with empty name
    profile.from = "";
    profile.to = "";
    profile.included_paths = [];
    profile.excluded_paths = [];
    profile.parallel = 16; // Default value
    profile.bandwidth = 5; // Default value

    // Create new ConfigInfo instance to avoid mutation
    const currentConfig = this.configInfo$.value;
    const updatedConfig = new models.ConfigInfo();
    Object.assign(updatedConfig, currentConfig);
    updatedConfig.profiles = [...currentConfig.profiles, profile];

    this.configInfo$.next(updatedConfig);

    // Save the new profile to the backend
    await this.saveConfigInfo();

    // Return the index of the newly created profile
    return updatedConfig.profiles.length - 1;
  }

  async removeProfile(index: number) {
    // Create new ConfigInfo instance to avoid mutation
    const currentConfig = this.configInfo$.value;
    const updatedConfig = new models.ConfigInfo();
    Object.assign(updatedConfig, currentConfig);
    updatedConfig.profiles = currentConfig.profiles.filter(
      (_, i) => i !== index
    );

    this.configInfo$.next(updatedConfig);

    // Save the changes to the backend
    await this.saveConfigInfo();
  }

  updateProfile(index: number, updatedProfile: models.Profile) {
    // Create new ConfigInfo instance to avoid mutation
    const currentConfig = this.configInfo$.value;
    const updatedConfig = new models.ConfigInfo();
    Object.assign(updatedConfig, currentConfig);

    // Deep clone profiles array and update specific profile
    updatedConfig.profiles = currentConfig.profiles.map((profile, i) => {
      if (i === index) {
        // Create a new Profile instance to avoid mutation
        const newProfile = new models.Profile();
        Object.assign(newProfile, updatedProfile);
        return newProfile;
      }
      return profile;
    });

    this.configInfo$.next(updatedConfig);
  }

  updateConfigInfo() {
    // This method is deprecated - use specific update methods instead
    // Keeping for backward compatibility but creating new instance
    const currentConfig = this.configInfo$.value;
    const updatedConfig = new models.ConfigInfo();
    Object.assign(updatedConfig, currentConfig);
    this.configInfo$.next(updatedConfig);
  }

  async saveConfigInfo() {
    try {
      await UpdateProfiles(this.configInfo$.value.profiles);
    } catch (e) {
      console.error(e);
      this.errorService.handleApiError(e, "save_config_info");
    }
  }

  addIncludePath(profileIndex: number) {
    const currentConfig = this.configInfo$.value;
    const updatedConfig = new models.ConfigInfo();
    Object.assign(updatedConfig, currentConfig);

    // Deep clone profiles array and update specific profile
    updatedConfig.profiles = currentConfig.profiles.map((profile, i) => {
      if (i === profileIndex) {
        const updatedProfile = new models.Profile();
        Object.assign(updatedProfile, profile);
        updatedProfile.included_paths = [...profile.included_paths, "/**"];
        return updatedProfile;
      }
      return profile;
    });

    this.configInfo$.next(updatedConfig);
  }

  removeIncludePath(profileIndex: number, index: number) {
    const currentConfig = this.configInfo$.value;
    const updatedConfig = new models.ConfigInfo();
    Object.assign(updatedConfig, currentConfig);

    // Deep clone profiles array and update specific profile
    updatedConfig.profiles = currentConfig.profiles.map((profile, i) => {
      if (i === profileIndex) {
        const updatedProfile = new models.Profile();
        Object.assign(updatedProfile, profile);
        updatedProfile.included_paths = profile.included_paths.filter(
          (_, pathIndex) => pathIndex !== index
        );
        return updatedProfile;
      }
      return profile;
    });

    this.configInfo$.next(updatedConfig);
  }

  addExcludePath(profileIndex: number) {
    const currentConfig = this.configInfo$.value;
    const updatedConfig = new models.ConfigInfo();
    Object.assign(updatedConfig, currentConfig);

    // Deep clone profiles array and update specific profile
    updatedConfig.profiles = currentConfig.profiles.map((profile, i) => {
      if (i === profileIndex) {
        const updatedProfile = new models.Profile();
        Object.assign(updatedProfile, profile);
        updatedProfile.excluded_paths = [...profile.excluded_paths, "/**"];
        return updatedProfile;
      }
      return profile;
    });

    this.configInfo$.next(updatedConfig);
  }

  removeExcludePath(profileIndex: number, index: number) {
    const currentConfig = this.configInfo$.value;
    const updatedConfig = new models.ConfigInfo();
    Object.assign(updatedConfig, currentConfig);

    // Deep clone profiles array and update specific profile
    updatedConfig.profiles = currentConfig.profiles.map((profile, i) => {
      if (i === profileIndex) {
        const updatedProfile = new models.Profile();
        Object.assign(updatedProfile, profile);
        updatedProfile.excluded_paths = profile.excluded_paths.filter(
          (_, pathIndex) => pathIndex !== index
        );
        return updatedProfile;
      }
      return profile;
    });

    this.configInfo$.next(updatedConfig);
  }

  // Import/Export methods
  async exportProfiles(): Promise<void> {
    try {
      await ExportProfiles("profiles_export.json");
    } catch (error) {
      console.error("Error exporting profiles:", error);
      this.errorService.handleApiError(error, "export_profiles");
      throw new Error("Failed to export profiles");
    }
  }

  async importProfiles(): Promise<void> {
    try {
      await ImportProfiles("profiles_import.json");
      await this.getConfigInfo(); // Refresh the profiles list
    } catch (error) {
      console.error("Error importing profiles:", error);
      this.errorService.handleApiError(error, "import_profiles");
      throw new Error("Failed to import profiles");
    }
  }

  async exportRemotes(): Promise<void> {
    try {
      await ExportRemotes("remotes_export.conf");
    } catch (error) {
      console.error("Error exporting remotes:", error);
      this.errorService.handleApiError(error, "export_remotes");
      throw new Error("Failed to export remotes");
    }
  }

  async importRemotes(): Promise<void> {
    try {
      await ImportRemotes("remotes_import.conf");
      await this.getRemotes(); // Refresh the remotes list
    } catch (error) {
      console.error("Error importing remotes:", error);
      this.errorService.handleApiError(error, "import_remotes");
      throw new Error("Failed to import remotes");
    }
  }
}
