import { inject, Injectable, NgZone, OnDestroy } from "@angular/core";
import { BehaviorSubject } from "rxjs";
import * as models from "../../../wailsjs/desktop/backend/models/models.js";
import {
  GetBoards,
  GetBoard,
  AddBoard,
  UpdateBoard,
  DeleteBoard,
  ExecuteBoard,
  StopBoardExecution,
  GetBoardExecutionStatus,
} from "../../../wailsjs/desktop/backend/services/boardservice.js";
import { Events } from "@wailsio/runtime";
import { parseEvent } from "../models/events.js";
import { isBoardEvent, isSyncEvent, type BoardEvent } from "../models/events.js";
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
  private readonly ngZone = inject(NgZone);
  private eventCleanup: (() => void) | undefined;

  constructor() {
    this.eventCleanup = Events.On("tofe", (event) => {
      const rawData = event.data;
      const parsedEvent = parseEvent(rawData as string);
      if (parsedEvent && isBoardEvent(parsedEvent)) {
        this.ngZone.run(() => this.handleBoardEvent(parsedEvent));
      } else if (parsedEvent && isSyncEvent(parsedEvent)) {
        this.ngZone.run(() => this.handleSyncLogEvent(parsedEvent));
      }
    });
  }

  ngOnDestroy(): void {
    this.eventCleanup?.();
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

      await ExecuteBoard(boardId);
      // Don't overwrite executionStatus$ from the RPC response —
      // events have already been updating it in real-time.
    } catch (err) {
      this.executionStatus$.next(null);
      this.errorService.handleApiError(err, "Failed to execute board");
    }
  }

  async stopExecution(boardId: string): Promise<void> {
    try {
      await StopBoardExecution(boardId);
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

  addNode(remoteName: string, path: string, label: string): void {
    const board = this.activeBoard$.value;
    if (!board) return;

    const node = new models.BoardNode();
    node.id = generateId("node");
    node.remote_name = remoteName;
    node.path = path;
    node.label = label || remoteName;
    node.x = 100 + Math.random() * 300;
    node.y = 100 + Math.random() * 300;

    board.nodes = [...(board.nodes || []), node];
    this.activeBoard$.next({ ...board });
  }

  removeNode(nodeId: string): void {
    const board = this.activeBoard$.value;
    if (!board) return;

    board.nodes = (board.nodes || []).filter((n) => n.id !== nodeId);
    // Remove connected edges
    board.edges = (board.edges || []).filter(
      (e) => e.source_id !== nodeId && e.target_id !== nodeId
    );
    this.activeBoard$.next({ ...board });
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
    }
  }

  addEdge(sourceId: string, targetId: string): void {
    const board = this.activeBoard$.value;
    if (!board) return;

    // Don't allow self-connections or duplicates
    if (sourceId === targetId) return;
    const existing = (board.edges || []).find(
      (e) => e.source_id === sourceId && e.target_id === targetId
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
    cfg.fast_list = true;
    edge.sync_config = cfg;

    board.edges = [...(board.edges || []), edge];
    this.activeBoard$.next({ ...board });
  }

  removeEdge(edgeId: string): void {
    const board = this.activeBoard$.value;
    if (!board) return;

    board.edges = (board.edges || []).filter((e) => e.id !== edgeId);
    this.activeBoard$.next({ ...board });
    this.selectedEdgeId$.next(null);
  }

  updateNodeInPlace(board: models.Board): void {
    this.activeBoard$.next({ ...board });
  }

  updateEdgeConfig(edgeId: string, updates: Partial<models.BoardEdge>): void {
    const board = this.activeBoard$.value;
    if (!board) return;

    const edge = (board.edges || []).find((e) => e.id === edgeId);
    if (edge) {
      Object.assign(edge, updates);
      this.activeBoard$.next({ ...board });
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

  private handleSyncLogEvent(event: import("../models/events.js").SyncEvent): void {
    const tabId = event.tabId;
    if (!tabId || !tabId.startsWith("board-")) return;

    // Parse tabId: "board-{boardId}-edge-{edgeId}"
    const match = tabId.match(/^board-(.+)-edge-(.+)$/);
    if (!match) return;

    const boardId = match[1];
    const edgeId = match[2];

    // Only capture logs for the active board
    const status = this.executionStatus$.value;
    if (!status || status.board_id !== boardId) return;

    const message = event.message || "";
    if (!message) return;

    // Accumulate per-edge log
    const existing = this.edgeLogs.get(edgeId) || "";
    this.edgeLogs.set(edgeId, existing ? existing + "\n" + message : message);

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
    const status = this.executionStatus$.value;
    if (!status || status.board_id !== event.boardId) return;

    // Update edge status if applicable
    if (event.edgeId && status.edge_statuses) {
      const edgeStatus = status.edge_statuses.find(
        (es) => es.edge_id === event.edgeId
      );
      if (edgeStatus) {
        edgeStatus.status = event.status;
        edgeStatus.message = event.message || "";
      }
    }

    // Update overall status
    const type = event.type as string;
    if (type === "board:execution:completed") {
      status.status = "completed";
    } else if (type === "board:execution:failed") {
      status.status = "failed";
    } else if (type === "board:execution:cancelled") {
      status.status = "cancelled";
    }

    this.executionStatus$.next({ ...status });
  }
}
