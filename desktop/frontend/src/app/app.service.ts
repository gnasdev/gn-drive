import { Injectable, OnDestroy, inject } from "@angular/core";
import { BehaviorSubject } from "rxjs";
// Import models from the new v3 bindings
import * as models from "../../wailsjs/desktop/backend/models/models.js";
import * as config from "../../wailsjs/github.com/rclone/rclone/fs/config/models.js";

// Import functions from the new v3 bindings
import { Events } from "@wailsio/runtime";
import {
    AddRemote,
    DeleteRemote,
    GetConfigInfo,
    GetRemotes,
    ReauthRemote,
    StopAddingRemote,
    StopCommand,
    Sync,
    SyncWithTabId,
    UpdateProfiles,
} from "../../wailsjs/desktop/backend/app";
import {
    CommandDTO,
    ConfigEvent,
    ErrorEvent,
    isConfigEvent,
    isErrorEvent,
    isLegacyCommandDTO,
    isSyncEvent,
    parseEvent,
    SyncEvent,
} from "./models/events";
import {
    DEFAULT_SYNC_STATUS,
    isValidSyncAction,
    isValidSyncStatus,
    SyncStatus,
    SyncStatusEvent,
} from "./models/sync-status.interface";
import { ErrorService } from "./services/error.service";
import { LogConsumerService } from "./services/log-consumer.service";
import { TabService } from "./tab.service";

// Legacy interface kept for backward compatibility
interface LegacyCommandDTO {
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
    private tabService = inject(TabService);
    private errorService = inject(ErrorService);
    private logConsumerService = inject(LogConsumerService);
    readonly currentId$ = new BehaviorSubject<number>(0);
    readonly currentAction$ = new BehaviorSubject<Action | undefined>(
        undefined,
    );
    readonly data$ = new BehaviorSubject<string[]>([]);
    readonly configInfo$: BehaviorSubject<models.ConfigInfo>;
    readonly remotes$ = new BehaviorSubject<config.Remote[]>([]);
    readonly syncStatus$ = new BehaviorSubject<SyncStatus | null>(null);

    private eventCleanup: (() => void) | undefined;

    constructor() {
        const configInfo = new models.ConfigInfo();
        configInfo.profiles = [];
        this.configInfo$ = new BehaviorSubject<models.ConfigInfo>(configInfo);
        // Store cleanup function for event listener
        this.eventCleanup = Events.On("tofe", (event) => {
            const rawData = event.data;
            const parsedEvent = parseEvent(rawData);
            if (!parsedEvent) {
                console.error("AppService: Failed to parse event");
                return;
            }
            // Handle new typed events
            if (isSyncEvent(parsedEvent)) {
                this.handleSyncEvent(parsedEvent);
                return;
            }

            if (isConfigEvent(parsedEvent)) {
                this.handleConfigEvent(parsedEvent);
                return;
            }

            if (isErrorEvent(parsedEvent)) {
                this.handleErrorEvent(parsedEvent);
                return;
            }

            // Handle legacy command DTOs
            if (isLegacyCommandDTO(parsedEvent)) {
                this.handleLegacyCommand(parsedEvent as LegacyCommandDTO);
                return;
            }

            // Fallback: try to handle as legacy command if it has command field
            if ("command" in parsedEvent) {
                this.handleLegacyCommand(
                    parsedEvent as unknown as LegacyCommandDTO,
                );
            }
        });
    }

    /**
     * Initialize data loading from backend.
     * Call this after first paint to avoid blocking the initial render.
     */
    initialize(): void {
        this.getConfigInfo();
        this.getRemotes();
    }

    ngOnDestroy() {
        // Cleanup event listener
        if (this.eventCleanup) {
            this.eventCleanup();
            this.eventCleanup = undefined;
        }
    }

    replaceData(str: string) {
        this.data$.next([str]);
    }

    private handleSyncStatusUpdate(statusEvent: SyncStatusEvent) {
        const currentStatus = this.syncStatus$.value || {
            ...DEFAULT_SYNC_STATUS,
        };

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
                statusEvent.files_transferred ??
                currentStatus.files_transferred,
            total_files: statusEvent.total_files ?? currentStatus.total_files,
            bytes_transferred:
                statusEvent.bytes_transferred ??
                currentStatus.bytes_transferred,
            total_bytes: statusEvent.total_bytes ?? currentStatus.total_bytes,
            current_file:
                statusEvent.current_file || currentStatus.current_file,
            errors: statusEvent.errors ?? currentStatus.errors,
            checks: statusEvent.checks ?? currentStatus.checks,
            deletes: statusEvent.deletes ?? currentStatus.deletes,
            renames: statusEvent.renames ?? currentStatus.renames,
            elapsed_time:
                statusEvent.elapsed_time || currentStatus.elapsed_time,
            action:
                statusEvent.action && isValidSyncAction(statusEvent.action)
                    ? statusEvent.action
                    : currentStatus.action,
        };

        this.syncStatus$.next(updatedStatus);
    }

    // Handle new typed sync events from backend
    private handleSyncEvent(event: SyncEvent) {
        // Route to LogConsumerService for reliable delivery with deduplication
        if (event.tabId && event.type === "sync:progress") {
            this.logConsumerService.handleLogEvent(event);
            return;
        }

        // Route other tab events to TabService
        if (event.tabId) {
            this.tabService.handleTypedSyncEvent(event);
            return;
        }

        // Handle global sync events
        switch (event.type) {
            case "sync:started":
                this.replaceData(`Sync started: ${event.message || ""}`);
                this.syncStatus$.next(null);
                break;
            case "sync:progress":
                // Progress updates are handled differently
                this.replaceData(`Sync in progress: ${event.message || ""}`);
                break;
            case "sync:completed":
                this.currentAction$.next(undefined);
                this.currentId$.next(0);
                this.replaceData(`Sync completed: ${event.message || ""}`);
                this.syncStatus$.next(null);
                break;
            case "sync:failed":
                this.currentAction$.next(undefined);
                this.currentId$.next(0);
                this.replaceData(`Sync failed: ${event.message || ""}`);
                this.syncStatus$.next(null);
                break;
            case "sync:cancelled":
                this.currentAction$.next(undefined);
                this.currentId$.next(0);
                this.replaceData("Sync cancelled");
                this.syncStatus$.next(null);
                break;
        }
    }

    // Handle config events from backend
    private handleConfigEvent(event: ConfigEvent) {
        // Refresh config info when config events are received
        switch (event.type) {
            case "config:updated":
            case "profile:added":
            case "profile:updated":
            case "profile:deleted":
                // Refresh config from backend to ensure consistency
                this.getConfigInfo();
                break;
        }
    }

    // Handle error events from backend
    private handleErrorEvent(event: ErrorEvent) {
        // Route to tab if tabId is present
        if (event.tabId) {
            const tab = this.tabService.getTab(event.tabId);
            if (tab) {
                this.tabService.updateTab(event.tabId, {
                    data: [...tab.data, `Error: ${event.message}`],
                });
            }
            return;
        }

        // Handle global error
        const dataValue = [...this.data$.value, `Error: ${event.message}`];
        this.data$.next(dataValue);

        // Also report to error service
        this.errorService.handleApiError(new Error(event.message), event.code);
    }

    // Handle legacy command DTOs for backward compatibility
    private handleLegacyCommand(data: LegacyCommandDTO) {
        // If event has tab_id, route to TabService
        if (data.tab_id) {
            this.tabService.handleCommandEvent(data as CommandDTO);
            return;
        }

        // Legacy handling for events without tab_id
        switch (data.command) {
            case "command_started":
                this.replaceData("Command started...");
                this.syncStatus$.next(null);
                break;
            case "command_stoped":
                this.currentAction$.next(undefined);
                this.currentId$.next(0);
                this.syncStatus$.next(null);
                break;
            case "command_output":
                this.replaceData(data.error || "");
                break;
            case "error": {
                const dataValue = [
                    ...this.data$.value,
                    data.error || "Unknown error",
                ];
                this.data$.next(dataValue);
                break;
            }
            case "sync_status":
                this.handleSyncStatusUpdate(data as unknown as SyncStatusEvent);
                break;
        }
    }

    async pull(profile: models.Profile) {
        if (this.currentAction$.value === Action.Pull) return;
        this.replaceData("Pulling...");

        try {
            const taskId = await Sync(Action.Pull, profile);
            this.currentId$.next(taskId);
            if (this.currentId$.value) {
                this.currentAction$.next(Action.Pull);
            } else {
                console.error("pull: No taskId returned from backend");
                this.replaceData("Error: Failed to start pull operation");
            }
        } catch (error) {
            console.error("pull error:", error);
            this.replaceData("Error: " + (error as Error).message);
        }
    }

    async pullWithTab(profile: models.Profile, tabId: string) {
        const tab = this.tabService.getTab(tabId);
        if (!tab || tab.currentAction === Action.Pull) return;
        this.tabService.updateTab(tabId, { data: ["Pulling..."] });

        try {
            const taskId = await SyncWithTabId(Action.Pull, profile, tabId);
            if (taskId) {
                this.tabService.updateTab(tabId, {
                    currentAction: Action.Pull,
                    currentTaskId: taskId,
                });
            } else {
                console.error("pullWithTab: No taskId returned from backend");
                this.tabService.updateTab(tabId, {
                    data: ["Error: Failed to start pull operation"],
                });
            }
        } catch (error) {
            console.error("pullWithTab error:", error);
            this.tabService.updateTab(tabId, {
                data: ["Error: " + (error as Error).message],
            });
        }
    }

    async push(profile: models.Profile) {
        if (this.currentAction$.value === Action.Push) return;
        this.replaceData("Pushing...");

        try {
            const taskId = await Sync(Action.Push, profile);
            this.currentId$.next(taskId);
            if (this.currentId$.value) {
                this.currentAction$.next(Action.Push);
            } else {
                console.error("push: No taskId returned from backend");
                this.replaceData("Error: Failed to start push operation");
            }
        } catch (error) {
            console.error("push error:", error);
            this.replaceData("Error: " + (error as Error).message);
        }
    }

    async pushWithTab(profile: models.Profile, tabId: string) {
        const tab = this.tabService.getTab(tabId);
        if (!tab || tab.currentAction === Action.Push) return;
        this.tabService.updateTab(tabId, { data: ["Pushing..."] });

        try {
            const taskId = await SyncWithTabId(Action.Push, profile, tabId);
            if (taskId) {
                this.tabService.updateTab(tabId, {
                    currentAction: Action.Push,
                    currentTaskId: taskId,
                });
            } else {
                console.error("pushWithTab: No taskId returned from backend");
                this.tabService.updateTab(tabId, {
                    data: ["Error: Failed to start push operation"],
                });
            }
        } catch (error) {
            console.error("pushWithTab error:", error);
            this.tabService.updateTab(tabId, {
                data: ["Error: " + (error as Error).message],
            });
        }
    }

    async bi(profile: models.Profile, resync = false) {
        if (this.currentAction$.value === Action.Bi) return;

        const action = resync ? Action.BiResync : Action.Bi;
        this.replaceData(resync ? "Resyncing..." : "Syncing...");

        try {
            const taskId = await Sync(action, profile);
            this.currentId$.next(taskId);
            if (this.currentId$.value) {
                this.currentAction$.next(Action.Bi);
            } else {
                console.error("bi: No taskId returned from backend");
                this.replaceData("Error: Failed to start sync operation");
            }
        } catch (error) {
            console.error("bi error:", error);
            this.replaceData("Error: " + (error as Error).message);
        }
    }

    async biWithTab(profile: models.Profile, tabId: string, resync = false) {
        const tab = this.tabService.getTab(tabId);
        if (!tab || tab.currentAction === Action.Bi) return;

        const action = resync ? Action.BiResync : Action.Bi;
        this.tabService.updateTab(tabId, {
            data: [resync ? "Resyncing..." : "Syncing..."],
        });

        try {
            const taskId = await SyncWithTabId(action, profile, tabId);
            if (taskId) {
                this.tabService.updateTab(tabId, {
                    currentAction: Action.Bi,
                    currentTaskId: taskId,
                });
            } else {
                console.error("biWithTab: No taskId returned from backend");
                this.tabService.updateTab(tabId, {
                    data: ["Error: Failed to start sync operation"],
                });
            }
        } catch (error) {
            console.error("biWithTab error:", error);
            this.tabService.updateTab(tabId, {
                data: ["Error: " + (error as Error).message],
            });
        }
    }

    stopCommand() {
        if (!this.currentAction$.value) return;
        try {
            StopCommand(this.currentId$.value);
        } catch (error) {
            console.error("stopCommand error:", error);
            this.replaceData(
                "Error stopping command: " + (error as Error).message,
            );
        }
    }

    stopCommandForTab(tabId: string) {
        const tab = this.tabService.getTab(tabId);
        if (!tab || !tab.currentAction || !tab.currentTaskId) return;
        try {
            StopCommand(tab.currentTaskId);
        } catch (error) {
            console.error("stopCommandForTab error:", error);
            this.tabService.updateTab(tabId, {
                data: [
                    ...tab.data,
                    "Error stopping command: " + (error as Error).message,
                ],
            });
        }
    }

    async getConfigInfo() {
        try {
            const configInfo = await GetConfigInfo();
            configInfo.profiles = configInfo.profiles ?? [];
            this.configInfo$.next(configInfo);
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

    async reauthRemote(name: string): Promise<void> {
        if (!name) {
            throw new Error("Remote name is required");
        }

        try {
            await ReauthRemote(name);
            await this.getRemotes();
        } catch (error) {
            console.error("Error re-authenticating remote:", error);
            this.errorService.handleApiError(error, "reauth_remote");
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
            (_, i) => i !== index,
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
                updatedProfile.included_paths = [
                    ...profile.included_paths,
                    "/**",
                ];
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
                    (_, pathIndex) => pathIndex !== index,
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
                updatedProfile.excluded_paths = [
                    ...profile.excluded_paths,
                    "/**",
                ];
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
                    (_, pathIndex) => pathIndex !== index,
                );
                return updatedProfile;
            }
            return profile;
        });

        this.configInfo$.next(updatedConfig);
    }
}
