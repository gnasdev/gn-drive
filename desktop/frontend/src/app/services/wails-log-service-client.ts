/** GN Drive note: Adapts generated Wails log bindings for Angular dependency injection. */
import { Provider } from "@angular/core";
import { GetLogsSince } from "../../../wailsjs/desktop/backend/services/logservice";
import {
    BackendLogEntry,
    LOG_SERVICE_CLIENT,
    LogServiceClient,
} from "./log-consumer.service";

const wailsLogServiceClient: LogServiceClient = {
    getLogsSince(
        tabId: string,
        afterSeqNo: number,
    ): PromiseLike<BackendLogEntry[]> {
        return GetLogsSince(tabId, afterSeqNo);
    },
};

export function provideWailsLogServiceClient(): Provider {
    return {
        provide: LOG_SERVICE_CLIENT,
        useValue: wailsLogServiceClient,
    };
}
