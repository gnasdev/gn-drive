import { Injectable } from "@angular/core";
import { BehaviorSubject } from "rxjs";
import { Action } from "./app.service";
import {
  SyncStatus,
  SyncStatusEvent,
  DEFAULT_SYNC_STATUS,
  isValidSyncStatus,
  isValidSyncAction,
} from "./models/sync-status.interface";
import { SyncEvent, CommandDTO } from "./models/events";

// Maximum number of output lines to keep per tab to prevent memory leaks
const MAX_OUTPUT_LINES = 1000;

export type OperationType = 'sync' | 'copy' | 'move' | 'check' | 'dedupe';
export type SyncDirection = 'pull' | 'push' | 'bi' | 'bi-resync';

export interface Tab {
  id: string;
  name: string;
  selectedProfileIndex: number | null;
  currentAction: Action | undefined;
  currentTaskId: number;
  data: string[];
  isActive: boolean;
  isEditing: boolean;
  isStopping?: boolean;
  syncStatus?: SyncStatus | null;
  operationType?: OperationType;
  syncDirection?: SyncDirection;
  dryRun?: boolean;
}

@Injectable({
  providedIn: "root",
})
export class TabService {
  private tabs$ = new BehaviorSubject<Tab[]>([]);
  private activeTabId$ = new BehaviorSubject<string | null>(null);

  constructor() {
    console.log("TabService constructor called");
  }

  get tabs() {
    return this.tabs$.asObservable();
  }

  get activeTabId() {
    return this.activeTabId$.asObservable();
  }

  get tabsValue() {
    return this.tabs$.value;
  }

  get activeTabIdValue() {
    return this.activeTabId$.value;
  }

  createTab(name?: string): string {
    const id = this.generateTabId();
    const tabName = name || `Tab ${this.tabs$.value.length + 1}`;

    const newTab: Tab = {
      id,
      name: tabName,
      selectedProfileIndex: null,
      currentAction: undefined,
      currentTaskId: 0,
      data: [],
      isActive: false,
      isEditing: false,
      isStopping: false,
      syncStatus: null,
    };

    const currentTabs = this.tabs$.value;

    // Set all tabs to inactive
    const updatedTabs = currentTabs.map((tab) => ({ ...tab, isActive: false }));

    // Add new tab as active
    newTab.isActive = true;
    updatedTabs.push(newTab);

    this.tabs$.next(updatedTabs);
    this.activeTabId$.next(id);

    return id;
  }

  deleteTab(tabId: string): void {
    const currentTabs = this.tabs$.value;
    const tabIndex = currentTabs.findIndex((tab) => tab.id === tabId);

    if (tabIndex === -1) return;

    const updatedTabs = currentTabs.filter((tab) => tab.id !== tabId);

    // If we deleted the active tab, set another tab as active
    if (this.activeTabId$.value === tabId) {
      if (updatedTabs.length > 0) {
        // Activate the previous tab, or the first one if we deleted the first tab
        const newActiveIndex = tabIndex > 0 ? tabIndex - 1 : 0;
        if (updatedTabs[newActiveIndex]) {
          updatedTabs[newActiveIndex].isActive = true;
          this.activeTabId$.next(updatedTabs[newActiveIndex].id);
        }
      } else {
        this.activeTabId$.next(null);
      }
    }

    this.tabs$.next(updatedTabs);
  }

  setActiveTab(tabId: string): void {
    const currentTabs = this.tabs$.value;
    const updatedTabs = currentTabs.map((tab) => ({
      ...tab,
      isActive: tab.id === tabId,
    }));

    this.tabs$.next(updatedTabs);
    this.activeTabId$.next(tabId);
  }

  updateTab(tabId: string, updates: Partial<Tab>): void {
    const currentTabs = this.tabs$.value;
    const updatedTabs = currentTabs.map((tab) =>
      tab.id === tabId ? { ...tab, ...updates } : tab
    );

    this.tabs$.next(updatedTabs);
  }

  getTab(tabId: string): Tab | undefined {
    return this.tabs$.value.find((tab) => tab.id === tabId);
  }

  getActiveTab(): Tab | undefined {
    const activeId = this.activeTabId$.value;
    if (!activeId) return undefined;
    return this.getTab(activeId);
  }

  startRenameTab(tabId: string): void {
    this.updateTab(tabId, { isEditing: true });
  }

  finishRenameTab(tabId: string, newName: string): void {
    const trimmedName = newName.trim();
    if (trimmedName) {
      this.updateTab(tabId, { name: trimmedName, isEditing: false });
    } else {
      this.updateTab(tabId, { isEditing: false });
    }
  }

  cancelRenameTab(tabId: string): void {
    this.updateTab(tabId, { isEditing: false });
  }

  handleCommandEvent(data: CommandDTO): void {
    console.log("TabService handleCommandEvent called with data:", data);

    if (!data.tab_id) {
      console.log("TabService: No tab_id in event data");
      return;
    }

    const tab = this.getTab(data.tab_id);
    if (!tab) {
      console.log("TabService: Tab not found for id:", data.tab_id);
      return;
    }

    console.log(
      "TabService: Processing command:",
      data.command,
      "for tab:",
      data.tab_id
    );
    switch (data.command) {
      case "command_started":
        // Clear previous data when a new command starts
        this.updateTab(data.tab_id, {
          data: ["Command started..."],
          syncStatus: null, // Reset sync status
        });
        break;
      case "command_stoped":
        this.updateTab(data.tab_id, {
          currentAction: undefined,
          currentTaskId: 0,
          isStopping: false,
          syncStatus: null, // Clear sync status
          data: this.limitOutput([...tab.data, "Command stopped."]),
        });
        break;
      case "command_output": {
        // Accumulate output during command execution
        this.updateTab(data.tab_id, {
          data: [data.error || ""],
        });
        break;
      }
      case "error":
        // Append errors to existing data with limit
        this.updateTab(data.tab_id, {
          data: this.limitOutput([...tab.data, data.error || ""]),
        });
        break;
      case "sync_status":
        // Handle sync status updates for tabs
        this.handleTabSyncStatusUpdate(data.tab_id, data as SyncStatusEvent);
        break;
    }
  }

  private handleTabSyncStatusUpdate(
    tabId: string,
    statusEvent: SyncStatusEvent
  ): void {
    const tab = this.getTab(tabId);
    if (!tab) return;

    const currentStatus = tab.syncStatus || { ...DEFAULT_SYNC_STATUS };

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

    this.updateTab(tabId, { syncStatus: updatedStatus });
  }

  // Handle new typed sync events from backend
  handleTypedSyncEvent(event: SyncEvent): void {
    if (!event.tabId) {
      console.log("TabService: No tabId in sync event");
      return;
    }

    const tab = this.getTab(event.tabId);
    if (!tab) {
      console.log("TabService: Tab not found for id:", event.tabId);
      return;
    }

    console.log("TabService: Processing typed sync event:", event.type, "for tab:", event.tabId);

    switch (event.type) {
      case "sync:started":
        this.updateTab(event.tabId, {
          data: [`Sync started: ${event.message || ""}`],
          syncStatus: null,
        });
        break;
      case "sync:progress":
        this.updateTab(event.tabId, {
          data: [`Sync in progress: ${event.message || ""}`],
        });
        break;
      case "sync:completed":
        this.updateTab(event.tabId, {
          currentAction: undefined,
          currentTaskId: 0,
          isStopping: false,
          syncStatus: null,
          data: this.limitOutput([...tab.data, `Sync completed: ${event.message || ""}`]),
        });
        break;
      case "sync:failed":
        this.updateTab(event.tabId, {
          currentAction: undefined,
          currentTaskId: 0,
          isStopping: false,
          syncStatus: null,
          data: this.limitOutput([...tab.data, `Sync failed: ${event.message || ""}`]),
        });
        break;
      case "sync:cancelled":
        this.updateTab(event.tabId, {
          currentAction: undefined,
          currentTaskId: 0,
          isStopping: false,
          syncStatus: null,
          data: this.limitOutput([...tab.data, "Sync cancelled"]),
        });
        break;
    }
  }

  private generateTabId(): string {
    return (
      "tab_" + Math.random().toString(36).substring(2, 11) + "_" + Date.now()
    );
  }

  // Limit output array size to prevent memory leaks
  private limitOutput(data: string[]): string[] {
    if (data.length <= MAX_OUTPUT_LINES) {
      return data;
    }
    // Keep the last MAX_OUTPUT_LINES entries
    return data.slice(-MAX_OUTPUT_LINES);
  }
}
