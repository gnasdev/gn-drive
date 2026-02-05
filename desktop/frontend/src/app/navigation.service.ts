import { Injectable } from "@angular/core";
import { BehaviorSubject } from "rxjs";

export type NavigationState =
    | { page: "board" }
    | { page: "remotes" }
    | { page: "settings" };

export type PageName = NavigationState["page"];

@Injectable({
    providedIn: "root",
})
export class NavigationService {
    private navigationState$ = new BehaviorSubject<NavigationState>({
        page: "board",
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

    navigateToBoard() {
        this.navigationState$.next({ page: "board" });
    }

    navigateToRemotes() {
        this.navigationState$.next({ page: "remotes" });
    }

    navigateToSettings() {
        this.navigationState$.next({ page: "settings" });
    }
}
