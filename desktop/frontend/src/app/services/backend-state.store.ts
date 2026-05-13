/** GN Drive note: Projects backend-owned app state into Angular observables. */
import { Injectable, OnDestroy } from "@angular/core";
import { Events } from "@wailsio/runtime";
import { BehaviorSubject } from "rxjs";
import * as models from "../../../wailsjs/desktop/backend/models/models.js";
import * as config from "../../../wailsjs/github.com/rclone/rclone/fs/config/models.js";
import { GetAppState } from "../../../wailsjs/desktop/backend/services/stateservice.js";
import {
    isStateEvent,
    parseEvent,
    type StateEvent,
} from "../models/events.js";

export interface BackendAppState {
    configInfo: models.ConfigInfo;
    remotes: config.Remote[];
    version: number;
}

@Injectable({
    providedIn: "root",
})
export class BackendStateStore implements OnDestroy {
    readonly configInfo$ = new BehaviorSubject<models.ConfigInfo>(
        this.emptyConfigInfo(),
    );
    readonly remotes$ = new BehaviorSubject<config.Remote[]>([]);
    readonly version$ = new BehaviorSubject<number>(0);

    private eventCleanup: (() => void) | undefined;
    private lastSeqNo = 0;

    constructor() {
        this.eventCleanup = Events.On("tofe", (event) => {
            const parsedEvent = parseEvent(event.data);
            if (!parsedEvent || !isStateEvent(parsedEvent)) return;
            this.applyStateEvent(parsedEvent);
        });
    }

    ngOnDestroy(): void {
        this.eventCleanup?.();
        this.eventCleanup = undefined;
    }

    async refresh(): Promise<void> {
        const state = await GetAppState();
        this.applySnapshot(state as BackendAppState);
    }

    private applyStateEvent(event: StateEvent): void {
        if (event.seqNo && event.seqNo <= this.lastSeqNo) return;
        this.lastSeqNo = event.seqNo || this.lastSeqNo;

        if (event.type === "state:snapshot") {
            this.applySnapshot(event.data as BackendAppState);
            return;
        }

        switch (event.slice) {
            case "config":
                this.applyConfigInfo(event.data as models.ConfigInfo);
                break;
            case "remotes":
                this.applyRemotes(event.data as config.Remote[]);
                break;
        }
    }

    private applySnapshot(state: BackendAppState | null | undefined): void {
        if (!state) return;
        this.applyConfigInfo(state.configInfo);
        this.applyRemotes(state.remotes);
        if ((state.version ?? 0) > this.lastSeqNo) {
            this.lastSeqNo = state.version;
        }
        this.version$.next(state.version ?? this.lastSeqNo);
    }

    private applyConfigInfo(configInfo: models.ConfigInfo | null | undefined): void {
        const nextConfig = configInfo ?? this.emptyConfigInfo();
        nextConfig.profiles = nextConfig.profiles ?? [];
        this.configInfo$.next(nextConfig);
    }

    private applyRemotes(remotes: config.Remote[] | null | undefined): void {
        this.remotes$.next(remotes ?? []);
    }

    private emptyConfigInfo(): models.ConfigInfo {
        const configInfo = new models.ConfigInfo();
        configInfo.profiles = [];
        return configInfo;
    }
}
