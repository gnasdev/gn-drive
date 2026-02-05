import { Injectable } from "@angular/core";
import { BehaviorSubject } from "rxjs";

export type NavigationState =
  | { page: "dashboard" }
  | { page: "operations" }
  | { page: "file-browser" }
  | { page: "board" }

  | { page: "remotes" }
  | { page: "schedules" }
  | { page: "history" }
  | { page: "settings" };

export type PageName = NavigationState["page"];

@Injectable({
  providedIn: "root",
})
export class NavigationService {
  private navigationState$ = new BehaviorSubject<NavigationState>({
    page: "dashboard",
  });

  get currentState$() {
    return this.navigationState$.asObservable();
  }

  get currentState() {
    return this.navigationState$.value;
  }

  navigateTo(page: PageName) {
    this.navigationState$.next({ page } as NavigationState);
  }

  navigateToDashboard() {
    this.navigationState$.next({ page: "dashboard" });
  }

  navigateToOperations() {
    this.navigationState$.next({ page: "operations" });
  }

  navigateToFileBrowser() {
    this.navigationState$.next({ page: "file-browser" });
  }

  navigateToBoard() {
    this.navigationState$.next({ page: "board" });
  }

  navigateToRemotes() {
    this.navigationState$.next({ page: "remotes" });
  }

  navigateToSchedules() {
    this.navigationState$.next({ page: "schedules" });
  }

  navigateToHistory() {
    this.navigationState$.next({ page: "history" });
  }

  navigateToSettings() {
    this.navigationState$.next({ page: "settings" });
  }

  // Backward compat
  navigateToHome() {
    this.navigateToOperations();
  }
}
