import { Injectable, OnDestroy } from "@angular/core";
import { BehaviorSubject, Subject } from "rxjs";
import { takeUntil } from "rxjs/operators";
import { Events } from "@wailsio/runtime";

// Import new services
import {
  SyncService,
  ConfigService,
  TabService,
} from "../../../wailsjs/desktop/backend/app";
import {
  Profile,
  SyncResult,
  Tab,
} from "../../../wailsjs/desktop/backend/models/models";

export interface SyncState {
  activeTasks: Record<number, any>;
  currentOperation: string | null;
  progress: number;
  status: string;
  error: string | null;
}

@Injectable({
  providedIn: "root",
})
export class NewSyncService implements OnDestroy {
  private destroy$ = new Subject<void>();

  // State management
  private syncState$ = new BehaviorSubject<SyncState>({
    activeTasks: {},
    currentOperation: null,
    progress: 0,
    status: "idle",
    error: null,
  });

  // Public observables
  readonly state$ = this.syncState$.asObservable();

  constructor() {
    this.setupEventListeners();
  }

  ngOnDestroy(): void {
    this.destroy$.next();
    this.destroy$.complete();
  }

  /**
   * Setup event listeners for backend events
   */
  private setupEventListeners(): void {
    // Listen for sync events
    Events.On("sync:started", (event) => {
      console.log("Sync started:", event);
      this.updateState({
        currentOperation: event.data?.action || "sync",
        status: "running",
        error: null,
      });
    });

    Events.On("sync:progress", (event) => {
      console.log("Sync progress:", event);
      this.updateState({
        progress: event.data?.progress || 0,
        status: "running",
      });
    });

    Events.On("sync:completed", (event) => {
      console.log("Sync completed:", event);
      this.updateState({
        currentOperation: null,
        status: "completed",
        progress: 100,
      });
    });

    Events.On("sync:failed", (event) => {
      console.log("Sync failed:", event);
      this.updateState({
        currentOperation: null,
        status: "failed",
        error: event.data?.message || "Sync operation failed",
      });
    });

    Events.On("sync:cancelled", (event) => {
      console.log("Sync cancelled:", event);
      this.updateState({
        currentOperation: null,
        status: "cancelled",
        error: null,
      });
    });

    // Listen for error events
    Events.On("error:occurred", (event) => {
      console.error("Backend error:", event);
      this.updateState({
        error: event.data?.message || "An error occurred",
      });
    });
  }

  /**
   * Update sync state
   */
  private updateState(updates: Partial<SyncState>): void {
    const currentState = this.syncState$.value;
    this.syncState$.next({
      ...currentState,
      ...updates,
    });
  }

  /**
   * Start a sync operation
   */
  async startSync(
    action: string,
    profile: Profile,
    tabId?: string
  ): Promise<SyncResult> {
    try {
      this.updateState({
        error: null,
        status: "starting",
      });

      const result = await SyncService.StartSync(action, profile, tabId);

      // Update active tasks
      const currentState = this.syncState$.value;
      this.updateState({
        activeTasks: {
          ...currentState.activeTasks,
          [result.taskId]: result,
        },
      });

      return result;
    } catch (error) {
      console.error("Failed to start sync:", error);
      this.updateState({
        status: "failed",
        error: error instanceof Error ? error.message : "Failed to start sync",
      });
      throw error;
    }
  }

  /**
   * Stop a sync operation
   */
  async stopSync(taskId: number): Promise<void> {
    try {
      await SyncService.StopSync(taskId);

      // Remove from active tasks
      const currentState = this.syncState$.value;
      const { [taskId]: removed, ...remainingTasks } = currentState.activeTasks;

      this.updateState({
        activeTasks: remainingTasks,
      });
    } catch (error) {
      console.error("Failed to stop sync:", error);
      this.updateState({
        error: error instanceof Error ? error.message : "Failed to stop sync",
      });
      throw error;
    }
  }

  /**
   * Get all active sync tasks
   */
  async getActiveTasks(): Promise<Record<number, any>> {
    try {
      const tasks = await SyncService.GetActiveTasks();
      this.updateState({
        activeTasks: tasks,
      });
      return tasks;
    } catch (error) {
      console.error("Failed to get active tasks:", error);
      throw error;
    }
  }

  /**
   * Pull operation (download from remote to local)
   */
  async pull(profile: Profile, tabId?: string): Promise<SyncResult> {
    return this.startSync("pull", profile, tabId);
  }

  /**
   * Push operation (upload from local to remote)
   */
  async push(profile: Profile, tabId?: string): Promise<SyncResult> {
    return this.startSync("push", profile, tabId);
  }

  /**
   * Bi-directional sync
   */
  async biSync(
    profile: Profile,
    tabId?: string,
    resync = false
  ): Promise<SyncResult> {
    const action = resync ? "bi-resync" : "bi";
    return this.startSync(action, profile, tabId);
  }

  /**
   * Clear error state
   */
  clearError(): void {
    this.updateState({
      error: null,
    });
  }

  /**
   * Reset sync state
   */
  reset(): void {
    this.syncState$.next({
      activeTasks: {},
      currentOperation: null,
      progress: 0,
      status: "idle",
      error: null,
    });
  }

  /**
   * Get current sync state
   */
  getCurrentState(): SyncState {
    return this.syncState$.value;
  }

  /**
   * Check if any sync operation is running
   */
  isRunning(): boolean {
    const state = this.syncState$.value;
    return state.status === "running" || state.status === "starting";
  }

  /**
   * Check if there are active tasks
   */
  hasActiveTasks(): boolean {
    const state = this.syncState$.value;
    return Object.keys(state.activeTasks).length > 0;
  }
}
