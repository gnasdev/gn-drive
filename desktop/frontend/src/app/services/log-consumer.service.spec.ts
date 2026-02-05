import { TestBed } from "@angular/core/testing";
import { SyncEvent } from "../models/events";
import { TabService } from "../tab.service";
import { LogConsumerService } from "./log-consumer.service";

describe("LogConsumerService", () => {
    let service: LogConsumerService;
    let tabService: TabService;

    beforeEach(() => {
        TestBed.configureTestingModule({
            providers: [LogConsumerService, TabService],
        });

        service = TestBed.inject(LogConsumerService);
        tabService = TestBed.inject(TabService);
    });

    afterEach(() => {
        service.stopAllPolling();
    });

    describe("handleLogEvent", () => {
        it("should append log message to tab", () => {
            const tabId = tabService.createTab("Test Tab");

            const event: SyncEvent = {
                type: "sync:progress",
                tabId,
                action: "pull",
                status: "running",
                message: "Syncing file.txt",
                seqNo: 1,
                timestamp: new Date().toISOString(),
            };

            service.handleLogEvent(event);

            const tab = tabService.getTab(tabId);
            expect(tab?.data).toContain("Syncing file.txt");
        });

        it("should deduplicate events with same seqNo", () => {
            const tabId = tabService.createTab("Test Tab");

            const event1: SyncEvent = {
                type: "sync:progress",
                tabId,
                action: "pull",
                status: "running",
                message: "Message 1",
                seqNo: 1,
                timestamp: new Date().toISOString(),
            };

            const event2: SyncEvent = {
                type: "sync:progress",
                tabId,
                action: "pull",
                status: "running",
                message: "Message 1 duplicate",
                seqNo: 1, // Same seqNo
                timestamp: new Date().toISOString(),
            };

            service.handleLogEvent(event1);
            service.handleLogEvent(event2);

            const tab = tabService.getTab(tabId);
            expect(tab?.data.length).toBe(1);
            expect(tab?.data[0]).toBe("Message 1");
        });

        it("should skip events with seqNo less than last received", () => {
            const tabId = tabService.createTab("Test Tab");

            const event1: SyncEvent = {
                type: "sync:progress",
                tabId,
                action: "pull",
                status: "running",
                message: "Message 1",
                seqNo: 5,
                timestamp: new Date().toISOString(),
            };

            const event2: SyncEvent = {
                type: "sync:progress",
                tabId,
                action: "pull",
                status: "running",
                message: "Old message",
                seqNo: 3, // Less than 5
                timestamp: new Date().toISOString(),
            };

            service.handleLogEvent(event1);
            service.handleLogEvent(event2);

            const tab = tabService.getTab(tabId);
            expect(tab?.data.length).toBe(1);
            expect(tab?.data[0]).toBe("Message 1");
        });

        it("should handle events without tabId", () => {
            const event: SyncEvent = {
                type: "sync:progress",
                tabId: undefined,
                action: "pull",
                status: "running",
                message: "Message",
                seqNo: 1,
                timestamp: new Date().toISOString(),
            };

            // Should not throw
            expect(() => service.handleLogEvent(event)).not.toThrow();
        });

        it("should handle events without seqNo (legacy)", () => {
            const tabId = tabService.createTab("Test Tab");

            const event: SyncEvent = {
                type: "sync:progress",
                tabId,
                action: "pull",
                status: "running",
                message: "Legacy message",
                timestamp: new Date().toISOString(),
            };

            service.handleLogEvent(event);

            const tab = tabService.getTab(tabId);
            expect(tab?.data).toContain("Legacy message");
        });

        it("should process events in order by seqNo", () => {
            const tabId = tabService.createTab("Test Tab");

            // Send events in order
            for (let i = 1; i <= 5; i++) {
                service.handleLogEvent({
                    type: "sync:progress",
                    tabId,
                    action: "pull",
                    status: "running",
                    message: `Message ${i}`,
                    seqNo: i,
                    timestamp: new Date().toISOString(),
                });
            }

            const tab = tabService.getTab(tabId);
            expect(tab?.data.length).toBe(5);
            expect(tab?.data[0]).toBe("Message 1");
            expect(tab?.data[4]).toBe("Message 5");
        });

        it("should not add empty messages", () => {
            const tabId = tabService.createTab("Test Tab");

            const event: SyncEvent = {
                type: "sync:progress",
                tabId,
                action: "pull",
                status: "running",
                message: "",
                seqNo: 1,
                timestamp: new Date().toISOString(),
            };

            service.handleLogEvent(event);

            const tab = tabService.getTab(tabId);
            expect(tab?.data.length).toBe(0);
        });
    });

    describe("resetTab", () => {
        it("should reset sequence tracking for a tab", () => {
            const tabId = tabService.createTab("Test Tab");

            // Set up some state
            service.handleLogEvent({
                type: "sync:progress",
                tabId,
                action: "pull",
                status: "running",
                message: "Message",
                seqNo: 10,
                timestamp: new Date().toISOString(),
            });

            expect(service.getLastSeqNo(tabId)).toBe(10);

            service.resetTab(tabId);

            expect(service.getLastSeqNo(tabId)).toBe(0);
        });
    });

    describe("getLastSeqNo", () => {
        it("should return 0 for unknown tab", () => {
            expect(service.getLastSeqNo("unknown-tab")).toBe(0);
        });

        it("should return last received seqNo for known tab", () => {
            const tabId = tabService.createTab("Test Tab");

            service.handleLogEvent({
                type: "sync:progress",
                tabId,
                action: "pull",
                status: "running",
                message: "Message",
                seqNo: 42,
                timestamp: new Date().toISOString(),
            });

            expect(service.getLastSeqNo(tabId)).toBe(42);
        });

        it("should track seqNo per tab independently", () => {
            const tabId1 = tabService.createTab("Tab 1");
            const tabId2 = tabService.createTab("Tab 2");

            service.handleLogEvent({
                type: "sync:progress",
                tabId: tabId1,
                action: "pull",
                status: "running",
                message: "Message",
                seqNo: 10,
                timestamp: new Date().toISOString(),
            });

            service.handleLogEvent({
                type: "sync:progress",
                tabId: tabId2,
                action: "pull",
                status: "running",
                message: "Message",
                seqNo: 20,
                timestamp: new Date().toISOString(),
            });

            expect(service.getLastSeqNo(tabId1)).toBe(10);
            expect(service.getLastSeqNo(tabId2)).toBe(20);
        });
    });

    describe("stopAllPolling", () => {
        it("should stop all polling timers", () => {
            const tabId1 = tabService.createTab("Tab 1");
            const tabId2 = tabService.createTab("Tab 2");

            service.startPolling(tabId1);
            service.startPolling(tabId2);

            // Should not throw
            expect(() => service.stopAllPolling()).not.toThrow();
        });
    });

    describe("limitOutput", () => {
        it("should limit output to MAX_OUTPUT_LINES (1000)", () => {
            const tabId = tabService.createTab("Test Tab");

            // Add more than 1000 messages
            for (let i = 1; i <= 1100; i++) {
                service.handleLogEvent({
                    type: "sync:progress",
                    tabId,
                    action: "pull",
                    status: "running",
                    message: `Message ${i}`,
                    seqNo: i,
                    timestamp: new Date().toISOString(),
                });
            }

            const tab = tabService.getTab(tabId);
            expect(tab?.data.length).toBeLessThanOrEqual(1000);

            // Should keep the most recent messages
            if (tab?.data) {
                expect(tab.data[tab.data.length - 1]).toBe("Message 1100");
            }
        });
    });
});
