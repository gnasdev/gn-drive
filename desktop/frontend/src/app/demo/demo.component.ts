import { Component, OnInit, OnDestroy } from "@angular/core";
import { CommonModule } from "@angular/common";
import { FormsModule } from "@angular/forms";
import { Subject } from "rxjs";
import { takeUntil } from "rxjs/operators";

import { NewSyncService, SyncState } from "../services/new-sync.service";
import {
  ConfigService,
  TabService,
} from "../../../wailsjs/desktop/backend/app";
import { Profile, Tab } from "../../../wailsjs/desktop/backend/models/models";

@Component({
  selector: "app-demo",
  standalone: true,
  imports: [CommonModule, FormsModule],
  template: `
    <div class="p-6 max-w-4xl mx-auto">
      <h1 class="text-2xl font-bold mb-6 text-gray-900 dark:text-gray-100">
        New Architecture Demo
      </h1>

      <!-- Sync State Display -->
      <div class="bg-white dark:bg-gray-800 rounded-lg shadow p-4 mb-6">
        <h2 class="text-lg font-semibold mb-4 text-gray-900 dark:text-gray-100">
          Sync State
        </h2>
        <div class="grid grid-cols-2 gap-4">
          <div>
            <label
              class="block text-sm font-medium text-gray-700 dark:text-gray-300"
              >Status:</label
            >
            <span class="text-sm" [class]="getStatusClass(syncState.status)">
              {{ syncState.status }}
            </span>
          </div>
          <div>
            <label
              class="block text-sm font-medium text-gray-700 dark:text-gray-300"
              >Progress:</label
            >
            <div class="w-full bg-gray-200 rounded-full h-2.5 dark:bg-gray-700">
              <div
                class="bg-blue-600 h-2.5 rounded-full transition-all duration-300"
                [style.width.%]="syncState.progress"
              ></div>
            </div>
            <span class="text-xs text-gray-500">{{ syncState.progress }}%</span>
          </div>
          <div>
            <label
              class="block text-sm font-medium text-gray-700 dark:text-gray-300"
              >Current Operation:</label
            >
            <span class="text-sm text-gray-900 dark:text-gray-100">
              {{ syncState.currentOperation || "None" }}
            </span>
          </div>
          <div>
            <label
              class="block text-sm font-medium text-gray-700 dark:text-gray-300"
              >Active Tasks:</label
            >
            <span class="text-sm text-gray-900 dark:text-gray-100">
              {{ Object.keys(syncState.activeTasks).length }}
            </span>
          </div>
        </div>

        <!-- Error Display -->
        <div
          *ngIf="syncState.error"
          class="mt-4 p-3 bg-red-100 border border-red-400 text-red-700 rounded"
        >
          <div class="flex justify-between items-center">
            <span>{{ syncState.error }}</span>
            <button
              (click)="clearError()"
              class="text-red-500 hover:text-red-700"
            >
              ×
            </button>
          </div>
        </div>
      </div>

      <!-- Demo Controls -->
      <div class="bg-white dark:bg-gray-800 rounded-lg shadow p-4 mb-6">
        <h2 class="text-lg font-semibold mb-4 text-gray-900 dark:text-gray-100">
          Demo Controls
        </h2>

        <!-- Profile Selection -->
        <div class="mb-4">
          <label
            class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2"
          >
            Select Profile:
          </label>
          <select
            [(ngModel)]="selectedProfileIndex"
            class="w-full p-2 border border-gray-300 rounded-md dark:bg-gray-700 dark:border-gray-600 dark:text-gray-100"
          >
            <option value="-1">No profile selected</option>
            <option *ngFor="let profile of profiles; let i = index" [value]="i">
              {{ profile.name }} ({{ profile.from }} → {{ profile.to }})
            </option>
          </select>
        </div>

        <!-- Tab Selection -->
        <div class="mb-4">
          <label
            class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2"
          >
            Select Tab (Optional):
          </label>
          <select
            [(ngModel)]="selectedTabId"
            class="w-full p-2 border border-gray-300 rounded-md dark:bg-gray-700 dark:border-gray-600 dark:text-gray-100"
          >
            <option value="">No tab selected</option>
            <option *ngFor="let tab of tabs" [value]="tab.id">
              {{ tab.name }} ({{ tab.state }})
            </option>
          </select>
        </div>

        <!-- Action Buttons -->
        <div class="flex flex-wrap gap-2">
          <button
            (click)="pull()"
            [disabled]="!canStartSync()"
            class="px-4 py-2 bg-blue-500 text-white rounded hover:bg-blue-600 disabled:opacity-50 disabled:cursor-not-allowed"
          >
            Pull
          </button>
          <button
            (click)="push()"
            [disabled]="!canStartSync()"
            class="px-4 py-2 bg-green-500 text-white rounded hover:bg-green-600 disabled:opacity-50 disabled:cursor-not-allowed"
          >
            Push
          </button>
          <button
            (click)="biSync()"
            [disabled]="!canStartSync()"
            class="px-4 py-2 bg-purple-500 text-white rounded hover:bg-purple-600 disabled:opacity-50 disabled:cursor-not-allowed"
          >
            Bi-Sync
          </button>
          <button
            (click)="biSyncResync()"
            [disabled]="!canStartSync()"
            class="px-4 py-2 bg-orange-500 text-white rounded hover:bg-orange-600 disabled:opacity-50 disabled:cursor-not-allowed"
          >
            Bi-Sync (Resync)
          </button>
          <button
            (click)="stopAllTasks()"
            [disabled]="!syncService.hasActiveTasks()"
            class="px-4 py-2 bg-red-500 text-white rounded hover:bg-red-600 disabled:opacity-50 disabled:cursor-not-allowed"
          >
            Stop All
          </button>
          <button
            (click)="refreshData()"
            class="px-4 py-2 bg-gray-500 text-white rounded hover:bg-gray-600"
          >
            Refresh Data
          </button>
        </div>
      </div>

      <!-- Active Tasks -->
      <div class="bg-white dark:bg-gray-800 rounded-lg shadow p-4 mb-6">
        <h2 class="text-lg font-semibold mb-4 text-gray-900 dark:text-gray-100">
          Active Tasks
        </h2>
        <div
          *ngIf="Object.keys(syncState.activeTasks).length === 0"
          class="text-gray-500 dark:text-gray-400"
        >
          No active tasks
        </div>
        <div
          *ngFor="let task of getActiveTasksArray()"
          class="border border-gray-200 dark:border-gray-600 rounded p-3 mb-2"
        >
          <div class="flex justify-between items-center">
            <div>
              <span class="font-medium">Task #{{ task.taskId }}</span>
              <span class="ml-2 text-sm text-gray-600 dark:text-gray-400">{{
                task.action
              }}</span>
              <span
                class="ml-2 text-sm"
                [class]="getStatusClass(task.status)"
                >{{ task.status }}</span
              >
            </div>
            <button
              (click)="stopTask(task.taskId)"
              class="px-3 py-1 bg-red-500 text-white text-sm rounded hover:bg-red-600"
            >
              Stop
            </button>
          </div>
          <div class="text-sm text-gray-600 dark:text-gray-400 mt-1">
            {{ task.message }}
          </div>
        </div>
      </div>

      <!-- Tabs -->
      <div class="bg-white dark:bg-gray-800 rounded-lg shadow p-4">
        <h2 class="text-lg font-semibold mb-4 text-gray-900 dark:text-gray-100">
          Tabs
        </h2>
        <div class="flex gap-2 mb-4">
          <input
            [(ngModel)]="newTabName"
            placeholder="Tab name"
            class="flex-1 p-2 border border-gray-300 rounded-md dark:bg-gray-700 dark:border-gray-600 dark:text-gray-100"
          />
          <button
            (click)="createTab()"
            [disabled]="!newTabName.trim()"
            class="px-4 py-2 bg-blue-500 text-white rounded hover:bg-blue-600 disabled:opacity-50 disabled:cursor-not-allowed"
          >
            Create Tab
          </button>
        </div>
        <div *ngIf="tabs.length === 0" class="text-gray-500 dark:text-gray-400">
          No tabs created
        </div>
        <div
          *ngFor="let tab of tabs"
          class="border border-gray-200 dark:border-gray-600 rounded p-3 mb-2"
        >
          <div class="flex justify-between items-center">
            <div>
              <span class="font-medium">{{ tab.name }}</span>
              <span class="ml-2 text-sm" [class]="getStatusClass(tab.state)">{{
                tab.state
              }}</span>
            </div>
            <button
              (click)="deleteTab(tab.id)"
              class="px-3 py-1 bg-red-500 text-white text-sm rounded hover:bg-red-600"
            >
              Delete
            </button>
          </div>
        </div>
      </div>
    </div>
  `,
  styles: [],
})
export class DemoComponent implements OnInit, OnDestroy {
  private destroy$ = new Subject<void>();

  syncState: SyncState = {
    activeTasks: {},
    currentOperation: null,
    progress: 0,
    status: "idle",
    error: null,
  };

  profiles: Profile[] = [];
  tabs: Tab[] = [];
  selectedProfileIndex = -1;
  selectedTabId = "";
  newTabName = "";

  constructor(public syncService: NewSyncService) {}

  ngOnInit(): void {
    // Subscribe to sync state changes
    this.syncService.state$
      .pipe(takeUntil(this.destroy$))
      .subscribe((state) => {
        this.syncState = state;
      });

    this.refreshData();
  }

  ngOnDestroy(): void {
    this.destroy$.next();
    this.destroy$.complete();
  }

  async refreshData(): Promise<void> {
    try {
      // Load profiles
      this.profiles = await ConfigService.GetProfiles();

      // Load tabs
      const tabsMap = await TabService.GetAllTabs();
      this.tabs = Object.values(tabsMap);

      // Refresh active tasks
      await this.syncService.getActiveTasks();
    } catch (error) {
      console.error("Failed to refresh data:", error);
    }
  }

  getSelectedProfile(): Profile | null {
    if (
      this.selectedProfileIndex >= 0 &&
      this.selectedProfileIndex < this.profiles.length
    ) {
      return this.profiles[this.selectedProfileIndex];
    }
    return null;
  }

  canStartSync(): boolean {
    return this.getSelectedProfile() !== null && !this.syncService.isRunning();
  }

  async pull(): Promise<void> {
    const profile = this.getSelectedProfile();
    if (!profile) return;

    try {
      await this.syncService.pull(profile, this.selectedTabId || undefined);
    } catch (error) {
      console.error("Pull failed:", error);
    }
  }

  async push(): Promise<void> {
    const profile = this.getSelectedProfile();
    if (!profile) return;

    try {
      await this.syncService.push(profile, this.selectedTabId || undefined);
    } catch (error) {
      console.error("Push failed:", error);
    }
  }

  async biSync(): Promise<void> {
    const profile = this.getSelectedProfile();
    if (!profile) return;

    try {
      await this.syncService.biSync(
        profile,
        this.selectedTabId || undefined,
        false
      );
    } catch (error) {
      console.error("Bi-sync failed:", error);
    }
  }

  async biSyncResync(): Promise<void> {
    const profile = this.getSelectedProfile();
    if (!profile) return;

    try {
      await this.syncService.biSync(
        profile,
        this.selectedTabId || undefined,
        true
      );
    } catch (error) {
      console.error("Bi-sync resync failed:", error);
    }
  }

  async stopAllTasks(): Promise<void> {
    const tasks = this.getActiveTasksArray();
    for (const task of tasks) {
      try {
        await this.syncService.stopSync(task.taskId);
      } catch (error) {
        console.error(`Failed to stop task ${task.taskId}:`, error);
      }
    }
  }

  async stopTask(taskId: number): Promise<void> {
    try {
      await this.syncService.stopSync(taskId);
    } catch (error) {
      console.error(`Failed to stop task ${taskId}:`, error);
    }
  }

  async createTab(): Promise<void> {
    if (!this.newTabName.trim()) return;

    try {
      await TabService.CreateTab(this.newTabName.trim());
      this.newTabName = "";
      await this.refreshData();
    } catch (error) {
      console.error("Failed to create tab:", error);
    }
  }

  async deleteTab(tabId: string): Promise<void> {
    try {
      await TabService.DeleteTab(tabId);
      await this.refreshData();
    } catch (error) {
      console.error("Failed to delete tab:", error);
    }
  }

  clearError(): void {
    this.syncService.clearError();
  }

  getActiveTasksArray(): any[] {
    return Object.values(this.syncState.activeTasks);
  }

  getStatusClass(status: string): string {
    switch (status) {
      case "running":
      case "starting":
        return "text-blue-600 dark:text-blue-400";
      case "completed":
        return "text-green-600 dark:text-green-400";
      case "failed":
        return "text-red-600 dark:text-red-400";
      case "cancelled":
        return "text-yellow-600 dark:text-yellow-400";
      case "idle":
        return "text-gray-600 dark:text-gray-400";
      default:
        return "text-gray-600 dark:text-gray-400";
    }
  }

  // Expose Object.keys for template
  Object = Object;
}
