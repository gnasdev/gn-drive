import { Injectable } from "@angular/core";
import { BehaviorSubject } from "rxjs";
import { Action } from "./app.service";

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
}

interface CommandDTO {
  command: string;
  pid: number | undefined;
  task: string | undefined;
  error: string | undefined;
  tab_id: string | undefined;
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
    if (!data.tab_id) return;

    const tab = this.getTab(data.tab_id);
    if (!tab) return;

    switch (data.command) {
      case "command_started":
        this.updateTab(data.tab_id, {
          data: ["Command started..."],
        });
        break;
      case "command_stoped":
        this.updateTab(data.tab_id, {
          currentAction: undefined,
          currentTaskId: 0,
          isStopping: false,
        });
        break;
      case "command_output":
        this.updateTab(data.tab_id, {
          data: [data.error || ""],
        });
        break;
      case "error":
        this.updateTab(data.tab_id, {
          data: [...tab.data, data.error || ""],
        });
        break;
    }
  }

  private generateTabId(): string {
    return (
      "tab_" + Math.random().toString(36).substring(2, 11) + "_" + Date.now()
    );
  }
}
