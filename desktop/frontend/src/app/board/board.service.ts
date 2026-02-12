import { inject, Injectable, OnDestroy } from "@angular/core";
import { Events } from "@wailsio/runtime";
import { BehaviorSubject, Subject, Subscription } from "rxjs";
import { debounceTime } from "rxjs/operators";
import * as models from "../../../wailsjs/desktop/backend/models/models.js";
import {
    AddBoard,
    ClearExecutionLogs,
    DeleteBoard,
    ExecuteBoard,
    GetBoard,
    GetBoardExecutionStatus,
    GetBoards,
    GetExecutionLogs,
    StopBoardExecution,
    UpdateBoard,
} from "../../../wailsjs/desktop/backend/services/boardservice.js";
import {
    isBoardEvent,
    isSyncEvent,
    parseEvent,
    type BoardEvent,
} from "../models/events.js";
import { ErrorService } from "../services/error.service.js";
import { generateId } from "./board.types.js";

@Injectable({
    providedIn: "root",
})
export class BoardService implements OnDestroy {
    readonly boards$ = new BehaviorSubject<models.Board[]>([]);
    readonly activeBoard$ = new BehaviorSubject<models.Board | null>(null);
    readonly executionStatus$ =
        new BehaviorSubject<models.BoardExecutionStatus | null>(null);

    // Canvas selection state
    readonly selectedNodeId$ = new BehaviorSubject<string | null>(null);
    readonly selectedEdgeId$ = new BehaviorSubject<string | null>(null);

    // Streaming logs
    readonly edgeLogs = new Map<string, string>();
    readonly latestLogLines$ = new BehaviorSubject<string>("");

    private readonly errorService = inject(ErrorService);
    private eventCleanup: (() => void) | undefined;
    private logPollingInterval: ReturnType<typeof setInterval> | null = null;
    private autoSaveSubscription: Subscription | null = null;
    private autoSaveTrigger$ = new Subject<void>();

    constructor() {
        this.eventCleanup = Events.On("tofe", (event) => {
            const rawData = event.data;
            const parsedEvent = parseEvent(rawData);
            if (parsedEvent && isBoardEvent(parsedEvent)) {
                this.handleBoardEvent(parsedEvent);
            } else if (parsedEvent && isSyncEvent(parsedEvent)) {
                this.handleSyncLogEvent(parsedEvent);
            }
        });

        // Auto-save active board on changes (debounced)
        this.autoSaveSubscription = this.autoSaveTrigger$
            .pipe(debounceTime(500))
            .subscribe(() => {
                const board = this.activeBoard$.value;
                if (board) {
                    this.persistBoard(board);
                }
            });
    }

    ngOnDestroy(): void {
        this.eventCleanup?.();
        this.stopLogPolling();
        this.autoSaveSubscription?.unsubscribe();
    }

    // Start polling for logs and status (workaround for Wails v3 event issues)
    private startLogPolling(): void {
        if (this.logPollingInterval) return;
        this.logPollingInterval = setInterval(async () => {
            try {
                // Poll for logs
                const logs = await GetExecutionLogs();
                if (logs && logs.length > 0) {
                    for (const logEntry of logs) {
                        this.appendLogEntry(logEntry);
                    }
                }

                // Also poll for execution status (fallback in case events are missed)
                const currentStatus = this.executionStatus$.value;
                if (currentStatus && currentStatus.status === "running") {
                    try {
                        const status = await GetBoardExecutionStatus(
                            currentStatus.board_id,
                        );
                        if (status && status.status !== "running") {
                            this.executionStatus$.next(status);
                            if (
                                status.status === "completed" ||
                                status.status === "failed" ||
                                status.status === "cancelled"
                            ) {
                                this.stopLogPolling();
                                if (status.status !== "cancelled") {
                                    this.fetchRemainingLogs();
                                }
                            }
                        }
                    } catch {
                        // If GetBoardExecutionStatus fails, the execution might have finished
                        // and been cleaned up from activeFlows
                        if (currentStatus) {
                            currentStatus.status = "completed";
                            this.executionStatus$.next({
                                ...currentStatus,
                            });
                        }
                        this.stopLogPolling();
                        this.fetchRemainingLogs();
                    }
                }
            } catch (err) {
                console.error("[BoardService] Error polling:", err);
            }
        }, 500);
    }

    // Stop polling for logs
    private stopLogPolling(): void {
        if (this.logPollingInterval) {
            clearInterval(this.logPollingInterval);
            this.logPollingInterval = null;
        }
    }

    // Append a single log entry to the latest lines
    private appendLogEntry(logEntry: string): void {
        const lines = logEntry.split("\n").filter((l) => l.trim());
        if (lines.length > 0) {
            const current = this.latestLogLines$.value;
            const allLines = current ? current.split("\n") : [];
            allLines.push(...lines);
            // Keep only last 5 lines
            const trimmed = allLines.slice(-5).join("\n");
            this.latestLogLines$.next(trimmed);
        }
    }

    async loadBoards(): Promise<void> {
        try {
            const boards = await GetBoards();
            this.boards$.next(boards || []);
        } catch (err) {
            this.errorService.handleApiError(err, "Failed to load boards");
        }
    }

    async setActiveBoard(boardId: string): Promise<void> {
        try {
            const board = await GetBoard(boardId);
            this.activeBoard$.next(board);
            this.selectedNodeId$.next(null);
            this.selectedEdgeId$.next(null);
        } catch (err) {
            this.errorService.handleApiError(err, "Failed to load board");
        }
    }

    async saveBoard(board: models.Board): Promise<void> {
        try {
            // Check if board already exists
            const existing = this.boards$.value.find((b) => b.id === board.id);
            if (existing) {
                await UpdateBoard(board);
            } else {
                await AddBoard(board);
            }
            await this.loadBoards();
            this.activeBoard$.next(board);
        } catch (err) {
            this.errorService.handleApiError(err, "Failed to save board");
        }
    }

    private async persistBoard(board: models.Board): Promise<void> {
        try {
            await UpdateBoard(board);
            await this.loadBoards();
        } catch (err) {
            this.errorService.handleApiError(err, "Failed to auto-save board");
        }
    }

    private triggerAutoSave(): void {
        this.autoSaveTrigger$.next();
    }

    async deleteBoard(boardId: string): Promise<void> {
        try {
            await DeleteBoard(boardId);
            if (this.activeBoard$.value?.id === boardId) {
                this.activeBoard$.next(null);
            }
            await this.loadBoards();
        } catch (err) {
            this.errorService.handleApiError(err, "Failed to delete board");
        }
    }

    async executeBoard(boardId: string): Promise<void> {
        try {
            this.edgeLogs.clear();
            this.latestLogLines$.next("");

            // Clear any existing logs in the backend buffer
            await ClearExecutionLogs();

            // Set preliminary execution status BEFORE the RPC call so that
            // events emitted by the backend goroutine (which starts immediately)
            // can be matched by handleBoardEvent / handleSyncLogEvent.
            const board = this.activeBoard$.value;
            if (board) {
                const prelimStatus = new models.BoardExecutionStatus();
                prelimStatus.board_id = boardId;
                prelimStatus.status = "running";
                prelimStatus.edge_statuses = (board.edges || []).map((e) => {
                    const es = new models.EdgeExecutionStatus();
                    es.edge_id = e.id;
                    es.status = "pending";
                    return es;
                });
                this.executionStatus$.next(prelimStatus);
            }

            // Start polling for logs (workaround for Wails v3 event issues)
            this.startLogPolling();

            // ExecuteBoard returns immediately (execution runs in background goroutine)
            // We rely on events to track progress and completion
            await ExecuteBoard(boardId);

            // Don't stop polling here - keep polling until we receive completion event
            // The polling will be stopped when handleBoardEvent receives completion/failed/cancelled
        } catch (err) {
            this.stopLogPolling();
            await ClearExecutionLogs();
            this.executionStatus$.next(null);
            this.errorService.handleApiError(err, "Failed to execute board");
        }
    }

    async stopExecution(boardId: string): Promise<void> {
        try {
            await StopBoardExecution(boardId);
            this.stopLogPolling();
        } catch (err) {
            this.errorService.handleApiError(err, "Failed to stop execution");
        }
    }

    async refreshExecutionStatus(boardId: string): Promise<void> {
        try {
            const status = await GetBoardExecutionStatus(boardId);
            this.executionStatus$.next(status);
        } catch {
            // Execution may have ended, clear status
            this.executionStatus$.next(null);
        }
    }

    // --- Canvas manipulation methods ---

    addNode(
        remoteName: string,
        path: string,
        label: string,
        x?: number,
        y?: number,
    ): void {
        const board = this.activeBoard$.value;
        if (!board) return;

        const node = new models.BoardNode();
        node.id = generateId("node");
        node.remote_name = remoteName;
        node.path = path;
        node.label = label || remoteName;
        node.x = x ?? 100 + Math.random() * 300;
        node.y = y ?? 100 + Math.random() * 300;

        board.nodes = [...(board.nodes || []), node];
        this.activeBoard$.next({ ...board });
        this.triggerAutoSave();
    }

    removeNode(nodeId: string): void {
        const board = this.activeBoard$.value;
        if (!board) return;

        board.nodes = (board.nodes || []).filter((n) => n.id !== nodeId);
        // Remove connected edges
        board.edges = (board.edges || []).filter(
            (e) => e.source_id !== nodeId && e.target_id !== nodeId,
        );
        this.activeBoard$.next({ ...board });
        this.triggerAutoSave();
        this.selectedNodeId$.next(null);
        this.selectedEdgeId$.next(null);
    }

    moveNode(nodeId: string, x: number, y: number): void {
        const board = this.activeBoard$.value;
        if (!board) return;

        const node = (board.nodes || []).find((n) => n.id === nodeId);
        if (node) {
            node.x = x;
            node.y = y;
            this.activeBoard$.next({ ...board });
            this.triggerAutoSave();
        }
    }

    addEdge(sourceId: string, targetId: string): void {
        const board = this.activeBoard$.value;
        if (!board) return;

        // Don't allow self-connections or duplicates
        if (sourceId === targetId) return;
        const existing = (board.edges || []).find(
            (e) => e.source_id === sourceId && e.target_id === targetId,
        );
        if (existing) return;

        const edge = new models.BoardEdge();
        edge.id = generateId("edge");
        edge.source_id = sourceId;
        edge.target_id = targetId;
        edge.action = "push";

        const cfg = new models.Profile();
        cfg.parallel = 8;
        cfg.bandwidth = 5;
        cfg.multi_thread_streams = 4;
        cfg.buffer_size = "16M";
        cfg.retries = 3;
        cfg.low_level_retries = 10;
        edge.sync_config = cfg;

        board.edges = [...(board.edges || []), edge];
        this.activeBoard$.next({ ...board });
        this.triggerAutoSave();
    }

    removeEdge(edgeId: string): void {
        const board = this.activeBoard$.value;
        if (!board) return;

        board.edges = (board.edges || []).filter((e) => e.id !== edgeId);
        this.activeBoard$.next({ ...board });
        this.triggerAutoSave();
        this.selectedEdgeId$.next(null);
    }

    reconnectEdge(
        edgeId: string,
        endpoint: "source" | "target",
        newNodeId: string,
    ): void {
        const board = this.activeBoard$.value;
        if (!board) return;

        const edge = (board.edges || []).find((e) => e.id === edgeId);
        if (!edge) return;

        // Prevent self-connections
        if (endpoint === "source" && newNodeId === edge.target_id) return;
        if (endpoint === "target" && newNodeId === edge.source_id) return;

        // Check for duplicate edge
        const newSourceId = endpoint === "source" ? newNodeId : edge.source_id;
        const newTargetId = endpoint === "target" ? newNodeId : edge.target_id;
        const duplicate = (board.edges || []).find(
            (e) =>
                e.id !== edgeId &&
                e.source_id === newSourceId &&
                e.target_id === newTargetId,
        );
        if (duplicate) return;

        // Update the edge endpoint
        if (endpoint === "source") {
            edge.source_id = newNodeId;
        } else {
            edge.target_id = newNodeId;
        }

        this.activeBoard$.next({ ...board });
        this.triggerAutoSave();
    }

    updateNodeInPlace(board: models.Board): void {
        this.activeBoard$.next({ ...board });
        this.triggerAutoSave();
    }

    updateEdgeConfig(edgeId: string, updates: Partial<models.BoardEdge>): void {
        const board = this.activeBoard$.value;
        if (!board) return;

        const edge = (board.edges || []).find((e) => e.id === edgeId);
        if (edge) {
            Object.assign(edge, updates);
            this.activeBoard$.next({ ...board });
            this.triggerAutoSave();
        }
    }

    selectNode(nodeId: string | null): void {
        this.selectedNodeId$.next(nodeId);
        this.selectedEdgeId$.next(null);
    }

    selectEdge(edgeId: string | null): void {
        this.selectedEdgeId$.next(edgeId);
        this.selectedNodeId$.next(null);
    }

    clearSelection(): void {
        this.selectedNodeId$.next(null);
        this.selectedEdgeId$.next(null);
    }

    getEdgeLog(edgeId: string): string {
        return this.edgeLogs.get(edgeId) || "";
    }

    // Handle execution started from external source (e.g., system tray)
    private async handleExternalExecutionStarted(
        boardId: string,
    ): Promise<void> {
        // Check if we already have an execution status for this board
        const currentStatus = this.executionStatus$.value;
        if (
            currentStatus &&
            currentStatus.board_id === boardId &&
            currentStatus.status === "running"
        ) {
            return; // Already tracking this execution
        }

        try {
            // Fetch the current execution status from backend
            const status = await GetBoardExecutionStatus(boardId);
            if (status) {
                this.edgeLogs.clear();
                this.latestLogLines$.next("");
                this.executionStatus$.next(status);
                this.startLogPolling();
            }
        } catch (err) {
            console.error(
                "Failed to get execution status for external execution:",
                err,
            );
        }
    }

    private handleSyncLogEvent(
        event: import("../models/events.js").SyncEvent,
    ): void {
        const tabId = event.tabId;
        if (!tabId || !tabId.startsWith("board-")) return;

        // Parse tabId: "{boardId}-{edgeId}" where boardId starts with "board-" and edgeId starts with "edge-"
        // Example: "board-123-xxx-edge-456-yyy" -> boardId="123-xxx", edgeId="456-yyy"
        const match = tabId.match(/^board-(.+)-edge-(.+)$/);
        if (!match) return;

        // Add prefixes back since the IDs in the model include them
        const boardId = `board-${match[1]}`;
        const edgeId = `edge-${match[2]}`;

        // Only capture logs for the active board
        const status = this.executionStatus$.value;
        if (!status || status.board_id !== boardId) return;

        const message = event.message || "";
        if (!message) return;

        // Accumulate per-edge log
        const existing = this.edgeLogs.get(edgeId) || "";
        this.edgeLogs.set(
            edgeId,
            existing ? existing + "\n" + message : message,
        );

        // Update latest log lines (last 3 lines across all edges)
        const lines = message.split("\n").filter((l) => l.trim());
        if (lines.length > 0) {
            const current = this.latestLogLines$.value;
            const allLines = current ? current.split("\n") : [];
            allLines.push(...lines);
            // Keep only last 3 lines
            const trimmed = allLines.slice(-3).join("\n");
            this.latestLogLines$.next(trimmed);
        }
    }

    private handleBoardEvent(event: BoardEvent): void {
        const type = event.type as string;

        // Handle board:updated with remote_deleted status - refresh boards
        if (type === "board:updated" && event.status === "remote_deleted") {
            this.loadBoards();
            return;
        }

        // Handle execution started event (e.g., triggered from system tray)
        if (type === "board:execution:started") {
            this.handleExternalExecutionStarted(event.boardId);
            return;
        }

        const status = this.executionStatus$.value;
        if (!status || status.board_id !== event.boardId) {
            return;
        }

        // Update edge status if applicable
        if (event.edgeId && status.edge_statuses) {
            const edgeStatus = status.edge_statuses.find(
                (es) => es.edge_id === event.edgeId,
            );
            if (edgeStatus) {
                edgeStatus.status = event.status;
                edgeStatus.message = event.message || "";
            }
        }

        // Update overall status
        if (type === "board:execution:completed") {
            status.status = "completed";
            this.stopLogPolling();
            this.fetchRemainingLogs();
        } else if (type === "board:execution:failed") {
            status.status = "failed";
            this.stopLogPolling();
            this.fetchRemainingLogs();
        } else if (type === "board:execution:cancelled") {
            status.status = "cancelled";
            this.stopLogPolling();
        }

        this.executionStatus$.next({ ...status });
    }

    private async fetchRemainingLogs(): Promise<void> {
        try {
            const remainingLogs = await GetExecutionLogs();
            if (remainingLogs && remainingLogs.length > 0) {
                for (const logEntry of remainingLogs) {
                    this.appendLogEntry(logEntry);
                }
            }
        } catch {
            // Ignore errors when fetching remaining logs
        }
    }
}
