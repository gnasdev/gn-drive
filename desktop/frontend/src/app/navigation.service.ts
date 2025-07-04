import { Injectable } from "@angular/core";
import { BehaviorSubject } from "rxjs";

export type NavigationState =
  | { page: "profiles" }
  | { page: "profile-edit"; profileIndex: number }
  | { page: "remotes" }
  | { page: "home" };

@Injectable({
  providedIn: "root",
})
export class NavigationService {
  private navigationState$ = new BehaviorSubject<NavigationState>({
    page: "home",
  });

  constructor() {
    console.log("NavigationService constructor called, initial state: home");
  }

  get currentState$() {
    return this.navigationState$.asObservable();
  }

  get currentState() {
    return this.navigationState$.value;
  }

  navigateToProfiles() {
    console.log("NavigationService navigateToProfiles called");
    this.navigationState$.next({ page: "profiles" });
  }

  navigateToProfileEdit(profileIndex: number) {
    console.log(
      "NavigationService navigateToProfileEdit called with index:",
      profileIndex
    );
    this.navigationState$.next({ page: "profile-edit", profileIndex });
  }

  navigateToRemotes() {
    console.log("NavigationService navigateToRemotes called");
    this.navigationState$.next({ page: "remotes" });
  }

  navigateToHome() {
    console.log("NavigationService navigateToHome called");
    this.navigationState$.next({ page: "home" });
  }
}
