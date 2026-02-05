import { inject, Injectable, OnDestroy } from "@angular/core";
import { GetLogsSince } from "../../../wailsjs/desktop/backend/services/logservice";
import { LogEntry } from "../../../wailsjs/desktop/backend/services/models";
import { SyncEvent } from "../models/events";
import { TabService } from "../tab.service";

// Maximum number of output lines to keep per tab
const MAX_OUTPUT_LINES = 1000;

@Injectable({
    providedIn: "root",
})
export class LogConsumerService implements OnDestroy {
    // Use inject() instead of constructor injection
    private tabService = inject(TabService);

    // Track last received sequence number per tab
    private lastSeqNo = new Map<string, number>();

    // Polling configuration
    private pollInterval = 2000; // 2 seconds
    private pollTimers = new Map<string, ReturnType<typeof setInterval>>();

    // Track if we're currently polling to avoid concurrent polls
    private isPolling = new Map<string, boolean>();

    ngOnDestroy(): void {
        // Clean up all polling timers
        this.pollTimers.forEach((timer) => clearInterval(timer));
        this.pollTimers.clear();
    }

    /**
     * Handle a log event from the backend
     * Implements deduplication via sequence numbers and gap detection
     */
    handleLogEvent(event: SyncEvent): void {
        if (!event.tabId) return;

        const seqNo = event.seqNo || 0;
        const lastSeq = this.lastSeqNo.get(event.tabId) || 0;

        // Dedupe: skip if already received
        if (seqNo > 0 && seqNo <= lastSeq) {
            return;
        }

        // Gap detection: if we missed some logs, trigger poll to recover them
        if (seqNo > 0 && seqNo > lastSeq + 1) {
            console.log(
                `[LogConsumer] Gap detected for tab ${event.tabId}: lastSeq=${lastSeq}, received=${seqNo}`,
            );
            this.pollMissedLogs(event.tabId, lastSeq);
        }

        // Append the log message
        if (event.message) {
            this.appendLog(event.tabId, event.message);
        }

        // Update last sequence number
        if (seqNo > 0) {
            this.lastSeqNo.set(event.tabId, seqNo);
        }
    }

    /**
     * Append a log message to a tab's output
     */
    private appendLog(tabId: string, message: string): void {
        const tab = this.tabService.getTab(tabId);
        if (!tab) return;

        const newData = this.limitOutput([...tab.data, message]);
        this.tabService.updateTab(tabId, { data: newData });
    }

    /**
     * Poll for missed logs from the backend
     */
    private async pollMissedLogs(
        tabId: string,
        afterSeqNo: number,
    ): Promise<void> {
        // Avoid concurrent polls for the same tab
        if (this.isPolling.get(tabId)) {
            return;
        }

        this.isPolling.set(tabId, true);

        try {
            const logs = await GetLogsSince(tabId, afterSeqNo);
            if (!logs || logs.length === 0) {
                return;
            }

            // Sort logs by sequence number
            logs.sort(
                (a: LogEntry, b: LogEntry) => (a.seqNo || 0) - (b.seqNo || 0),
            );

            // Process each log entry
            for (const log of logs) {
                const logSeqNo = log.seqNo || 0;
                const currentLastSeq = this.lastSeqNo.get(tabId) || 0;

                // Skip if already processed
                if (logSeqNo <= currentLastSeq) {
                    continue;
                }

                // Append the log
                if (log.message) {
                    this.appendLog(tabId, log.message);
                }

                // Update last sequence number
                this.lastSeqNo.set(tabId, logSeqNo);
            }

            console.log(
                `[LogConsumer] Recovered ${logs.length} logs for tab ${tabId}`,
            );
        } catch (error) {
            console.error(
                `[LogConsumer] Failed to poll logs for tab ${tabId}:`,
                error,
            );
        } finally {
            this.isPolling.set(tabId, false);
        }
    }

    /**
     * Start periodic polling for a tab (fallback mechanism)
     */
    startPolling(tabId: string): void {
        // Don't start if already polling
        if (this.pollTimers.has(tabId)) {
            return;
        }

        const timer = setInterval(() => {
            const lastSeq = this.lastSeqNo.get(tabId) || 0;
            this.pollMissedLogs(tabId, lastSeq);
        }, this.pollInterval);

        this.pollTimers.set(tabId, timer);
    }

    /**
     * Stop polling for a tab
     */
    stopPolling(tabId: string): void {
        const timer = this.pollTimers.get(tabId);
        if (timer) {
            clearInterval(timer);
            this.pollTimers.delete(tabId);
        }
    }

    /**
     * Stop polling for all tabs
     */
    stopAllPolling(): void {
        this.pollTimers.forEach((timer) => clearInterval(timer));
        this.pollTimers.clear();
    }

    /**
     * Reset state for a tab (e.g., when starting a new sync)
     */
    resetTab(tabId: string): void {
        this.lastSeqNo.delete(tabId);
        this.isPolling.delete(tabId);
    }

    /**
     * Get the last sequence number for a tab
     */
    getLastSeqNo(tabId: string): number {
        return this.lastSeqNo.get(tabId) || 0;
    }

    /**
     * Limit output array size to prevent memory leaks
     */
    private limitOutput(data: string[]): string[] {
        if (data.length <= MAX_OUTPUT_LINES) {
            return data;
        }
        // Keep the last MAX_OUTPUT_LINES entries
        return data.slice(-MAX_OUTPUT_LINES);
    }
}
