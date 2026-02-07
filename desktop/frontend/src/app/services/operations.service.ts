import { inject, Injectable, NgZone, OnDestroy } from '@angular/core';
import { Events } from '@wailsio/runtime';
import { BehaviorSubject, Subject, Subscription } from 'rxjs';
import { debounceTime } from 'rxjs/operators';
import * as models from '../../../wailsjs/desktop/backend/models/models.js';
import {
  AddBoard,
  ClearExecutionLogs,
  DeleteBoard,
  ExecuteBoard,
  GetBoards,
  GetExecutionLogs,
  StopBoardExecution,
  UpdateBoard,
} from '../../../wailsjs/desktop/backend/services/boardservice.js';
import { isSyncEvent, parseEvent, type SyncEvent } from '../models/events.js';
import { ErrorService } from './error.service.js';
import {
  appendOperationLog,
  createEmptyOperation,
  findOperationById,
  flattenOperations,
  generateId,
  MAX_LOG_LINES,
  Operation,
  OperationsState,
  SyncConfig,
} from '../models/operation.model.js';

@Injectable({
  providedIn: 'root',
})
export class OperationsService implements OnDestroy {
  private readonly errorService = inject(ErrorService);
  private readonly ngZone = inject(NgZone);

  // State
  readonly state$ = new BehaviorSubject<OperationsState>({
    operations: [],
  });

  // Active execution tracking
  readonly activeOperationId$ = new BehaviorSubject<string | null>(null);

  private eventCleanup: (() => void) | undefined;
  private logPollingInterval: ReturnType<typeof setInterval> | null = null;
  private autoSaveSubscription: Subscription | null = null;
  private autoSaveTrigger$ = new Subject<void>();

  // Map operation IDs to board edge IDs for execution tracking
  private operationToBoardMap = new Map<string, { boardId: string; edgeId: string }>();

  constructor() {
    this.eventCleanup = Events.On('tofe', (event) => {
      const rawData = event.data;
      const parsedEvent = parseEvent(rawData);
      if (parsedEvent && isSyncEvent(parsedEvent)) {
        this.ngZone.run(() => this.handleSyncLogEvent(parsedEvent));
      }
    });

    // Auto-save on changes (debounced)
    this.autoSaveSubscription = this.autoSaveTrigger$.pipe(debounceTime(500)).subscribe(() => {
      this.persistOperations();
    });
  }

  ngOnDestroy(): void {
    this.eventCleanup?.();
    this.stopLogPolling();
    this.autoSaveSubscription?.unsubscribe();
  }

  /**
   * Load operations from backend (transforms boards to operations)
   */
  async loadOperations(): Promise<void> {
    try {
      const boards = await GetBoards();
      const operations = this.boardsToOperations(boards || []);
      this.state$.next({ ...this.state$.value, operations });
    } catch (err) {
      this.errorService.handleApiError(err, 'Failed to load operations');
    }
  }

  /**
   * Get all operations (flat list)
   */
  getOperations(): Operation[] {
    return this.state$.value.operations;
  }

  /**
   * Add a new operation
   */
  addOperation(parentId?: string): Operation {
    const operation = createEmptyOperation();
    const state = this.state$.value;

    if (parentId) {
      const parent = findOperationById(state.operations, parentId);
      if (parent) {
        parent.children = parent.children || [];
        parent.children.push(operation);
        operation.parentId = parentId;
      }
    } else {
      state.operations.push(operation);
    }

    this.state$.next({ ...state });
    this.triggerAutoSave();
    return operation;
  }

  /**
   * Update an operation
   */
  updateOperation(operationId: string, updates: Partial<Operation>): void {
    const state = this.state$.value;
    const operation = findOperationById(state.operations, operationId);
    if (operation) {
      Object.assign(operation, updates);
      this.state$.next({ ...state });
      this.triggerAutoSave();
    }
  }

  /**
   * Remove an operation
   */
  removeOperation(operationId: string): void {
    const state = this.state$.value;
    this.removeOperationFromTree(state.operations, operationId);
    this.state$.next({ ...state });
    this.triggerAutoSave();
  }

  /**
   * Move operation (for drag-drop)
   */
  moveOperation(
    operationId: string,
    targetId: string,
    position: 'before' | 'after' | 'inside'
  ): void {
    const state = this.state$.value;

    // Remove from current location
    const operation = findOperationById(state.operations, operationId);
    if (!operation) return;

    this.removeOperationFromTree(state.operations, operationId);

    // Insert at new location
    if (position === 'inside') {
      const target = findOperationById(state.operations, targetId);
      if (target) {
        target.children = target.children || [];
        target.children.push(operation);
        operation.parentId = targetId;
        // Change target to sequential/parallel group if it was single
        if (target.groupType === 'single') {
          target.groupType = 'sequential';
        }
      }
    } else {
      // Insert before or after target at same level
      const insertResult = this.insertOperationAtPosition(
        state.operations,
        operation,
        targetId,
        position
      );
      if (!insertResult) {
        // If not found at root, try to find parent
        const allOps = flattenOperations(state.operations);
        for (const op of allOps) {
          if (op.children) {
            const inserted = this.insertOperationAtPosition(
              op.children,
              operation,
              targetId,
              position
            );
            if (inserted) {
              operation.parentId = op.id;
              break;
            }
          }
        }
      } else {
        operation.parentId = undefined;
      }
    }

    this.state$.next({ ...state });
    this.triggerAutoSave();
  }

  /**
   * Execute a single operation
   */
  async executeOperation(operationId: string): Promise<void> {
    const state = this.state$.value;
    const operation = findOperationById(state.operations, operationId);
    if (!operation) return;

    // Clear logs and set status
    operation.logs = [];
    operation.status = 'running';
    operation.isLogsVisible = true;
    this.activeOperationId$.next(operationId);
    this.state$.next({ ...state });

    try {
      // Convert operation to a temporary board for execution
      const board = this.operationToBoard(operation);
      await AddBoard(board);

      // Store mapping for log tracking
      const edgeId = board.edges?.[0]?.id;
      if (edgeId) {
        this.operationToBoardMap.set(operationId, { boardId: board.id, edgeId });
      }

      // Clear logs and start polling
      await ClearExecutionLogs();
      this.startLogPolling(operationId);

      // Execute
      await ExecuteBoard(board.id);
    } catch (err) {
      operation.status = 'failed';
      this.activeOperationId$.next(null);
      this.state$.next({ ...state });
      this.errorService.handleApiError(err, 'Failed to execute operation');
    }
  }

  /**
   * Stop executing an operation
   */
  async stopExecution(operationId: string): Promise<void> {
    const mapping = this.operationToBoardMap.get(operationId);
    if (mapping) {
      try {
        await StopBoardExecution(mapping.boardId);
      } catch (err) {
        this.errorService.handleApiError(err, 'Failed to stop execution');
      }
    }

    const state = this.state$.value;
    const operation = findOperationById(state.operations, operationId);
    if (operation) {
      operation.status = 'cancelled';
      this.state$.next({ ...state });
    }

    this.stopLogPolling();
    this.activeOperationId$.next(null);
    this.operationToBoardMap.delete(operationId);
  }

  /**
   * Toggle settings panel visibility
   */
  toggleSettings(operationId: string): void {
    const state = this.state$.value;
    const operation = findOperationById(state.operations, operationId);
    if (operation) {
      operation.isExpanded = !operation.isExpanded;
      this.state$.next({ ...state });
    }
  }

  /**
   * Toggle logs panel visibility
   */
  toggleLogs(operationId: string): void {
    const state = this.state$.value;
    const operation = findOperationById(state.operations, operationId);
    if (operation) {
      operation.isLogsVisible = !operation.isLogsVisible;
      this.state$.next({ ...state });
    }
  }

  // --- Private methods ---

  private triggerAutoSave(): void {
    this.autoSaveTrigger$.next();
  }

  private async persistOperations(): Promise<void> {
    try {
      const operations = this.state$.value.operations;

      // Convert operations to boards and save
      // For now, we save all operations as a single "Operations" board
      const existingBoards = await GetBoards();
      const operationsBoard = existingBoards?.find((b) => b.name === '__operations__');

      const board = this.operationsToBoard(operations);
      board.name = '__operations__';

      if (operationsBoard) {
        board.id = operationsBoard.id;
        await UpdateBoard(board);
      } else {
        await AddBoard(board);
      }
    } catch (err) {
      this.errorService.handleApiError(err, 'Failed to save operations');
    }
  }

  private removeOperationFromTree(operations: Operation[], id: string): boolean {
    const index = operations.findIndex((op) => op.id === id);
    if (index !== -1) {
      operations.splice(index, 1);
      return true;
    }

    for (const op of operations) {
      if (op.children && this.removeOperationFromTree(op.children, id)) {
        return true;
      }
    }

    return false;
  }

  private insertOperationAtPosition(
    operations: Operation[],
    operation: Operation,
    targetId: string,
    position: 'before' | 'after'
  ): boolean {
    const index = operations.findIndex((op) => op.id === targetId);
    if (index !== -1) {
      const insertIndex = position === 'before' ? index : index + 1;
      operations.splice(insertIndex, 0, operation);
      return true;
    }
    return false;
  }

  private startLogPolling(operationId: string): void {
    if (this.logPollingInterval) return;

    this.logPollingInterval = setInterval(async () => {
      try {
        const logs = await GetExecutionLogs();
        if (logs && logs.length > 0) {
          this.ngZone.run(() => {
            const state = this.state$.value;
            const operation = findOperationById(state.operations, operationId);
            if (operation) {
              for (const logEntry of logs) {
                appendOperationLog(operation, logEntry);
              }
              this.state$.next({ ...state });
            }
          });
        }
      } catch (err) {
        console.error('[OperationsService] Error polling logs:', err);
      }
    }, 500);
  }

  private stopLogPolling(): void {
    if (this.logPollingInterval) {
      clearInterval(this.logPollingInterval);
      this.logPollingInterval = null;
    }
  }

  private handleSyncLogEvent(event: SyncEvent): void {
    const activeOpId = this.activeOperationId$.value;
    if (!activeOpId) return;

    const mapping = this.operationToBoardMap.get(activeOpId);
    if (!mapping) return;

    const tabId = event.tabId;
    if (!tabId || !tabId.includes(mapping.boardId)) return;

    const message = event.message || '';
    if (!message) return;

    const state = this.state$.value;
    const operation = findOperationById(state.operations, activeOpId);
    if (operation) {
      appendOperationLog(operation, message);
      this.state$.next({ ...state });
    }

    // Check for completion events
    if (event.type === 'sync:completed' || event.type === 'sync:failed') {
      operation!.status = event.type === 'sync:completed' ? 'completed' : 'failed';
      this.stopLogPolling();
      this.activeOperationId$.next(null);
      this.operationToBoardMap.delete(activeOpId);

      // Cleanup temporary board
      this.cleanupTemporaryBoard(mapping.boardId);
    }
  }

  private async cleanupTemporaryBoard(boardId: string): Promise<void> {
    try {
      await DeleteBoard(boardId);
    } catch {
      // Ignore cleanup errors
    }
  }

  // --- Transform methods ---

  /**
   * Convert boards to operations tree
   */
  private boardsToOperations(boards: models.Board[]): Operation[] {
    // Look for the special __operations__ board
    const operationsBoard = boards.find((b) => b.name === '__operations__');
    if (!operationsBoard) return [];

    return this.boardEdgesToOperations(operationsBoard);
  }

  /**
   * Convert a board's edges to operations
   */
  private boardEdgesToOperations(board: models.Board): Operation[] {
    const operations: Operation[] = [];
    const nodes = board.nodes || [];
    const edges = board.edges || [];

    for (const edge of edges) {
      const sourceNode = nodes.find((n) => n.id === edge.source_id);
      const targetNode = nodes.find((n) => n.id === edge.target_id);

      if (!sourceNode || !targetNode) continue;

      const operation: Operation = {
        id: edge.id,
        sourceRemote: sourceNode.remote_name,
        sourcePath: sourceNode.path || '/',
        targetRemote: targetNode.remote_name,
        targetPath: targetNode.path || '/',
        groupType: 'single',
        syncConfig: this.profileToSyncConfig(edge.sync_config, edge.action),
        scheduleEnabled: false,
        status: 'idle',
        logs: [],
        isExpanded: false,
        isLogsVisible: false,
      };

      operations.push(operation);
    }

    // Build tree structure based on parentId
    return this.buildOperationTree(operations);
  }

  /**
   * Build tree from flat list with parentIds
   */
  private buildOperationTree(operations: Operation[]): Operation[] {
    const map = new Map<string, Operation>();
    const roots: Operation[] = [];

    for (const op of operations) {
      map.set(op.id, op);
    }

    for (const op of operations) {
      if (op.parentId && map.has(op.parentId)) {
        const parent = map.get(op.parentId)!;
        parent.children = parent.children || [];
        parent.children.push(op);
      } else {
        roots.push(op);
      }
    }

    return roots;
  }

  /**
   * Convert Profile to SyncConfig
   */
  private profileToSyncConfig(profile: models.Profile | undefined, action: string): SyncConfig {
    return {
      action: (action as SyncConfig['action']) || 'push',
      parallel: profile?.parallel,
      bandwidth: profile?.bandwidth ? `${profile.bandwidth}M` : undefined,
      includedPaths: profile?.included_paths,
      excludedPaths: profile?.excluded_paths,
    };
  }

  /**
   * Convert operations tree to a board
   */
  private operationsToBoard(operations: Operation[]): models.Board {
    const board = new models.Board();
    board.id = generateId();
    board.name = '__operations__';
    board.nodes = [];
    board.edges = [];

    const nodeMap = new Map<string, string>(); // remote:path -> nodeId

    const flatOps = flattenOperations(operations);

    for (const op of flatOps) {
      // Create source node if not exists
      const sourceKey = `${op.sourceRemote}:${op.sourcePath}`;
      if (!nodeMap.has(sourceKey)) {
        const node = new models.BoardNode();
        node.id = generateId();
        node.remote_name = op.sourceRemote;
        node.path = op.sourcePath;
        node.label = op.sourceRemote;
        node.x = 100;
        node.y = board.nodes.length * 100 + 100;
        board.nodes.push(node);
        nodeMap.set(sourceKey, node.id);
      }

      // Create target node if not exists
      const targetKey = `${op.targetRemote}:${op.targetPath}`;
      if (!nodeMap.has(targetKey)) {
        const node = new models.BoardNode();
        node.id = generateId();
        node.remote_name = op.targetRemote;
        node.path = op.targetPath;
        node.label = op.targetRemote;
        node.x = 400;
        node.y = board.nodes.length * 100 + 100;
        board.nodes.push(node);
        nodeMap.set(targetKey, node.id);
      }

      // Create edge
      const edge = new models.BoardEdge();
      edge.id = op.id;
      edge.source_id = nodeMap.get(sourceKey)!;
      edge.target_id = nodeMap.get(targetKey)!;
      edge.action = op.syncConfig.action;

      // Convert SyncConfig to Profile
      const profile = new models.Profile();
      profile.parallel = op.syncConfig.parallel || 8;
      profile.bandwidth = op.syncConfig.bandwidth
        ? parseInt(op.syncConfig.bandwidth.replace('M', ''))
        : 0;
      profile.included_paths = op.syncConfig.includedPaths || [];
      profile.excluded_paths = op.syncConfig.excludedPaths || [];

      edge.sync_config = profile;
      board.edges.push(edge);
    }

    return board;
  }

  /**
   * Convert a single operation to a board for execution
   */
  private operationToBoard(operation: Operation): models.Board {
    const board = new models.Board();
    board.id = `temp-${generateId()}`;
    board.name = `__temp_${operation.id}__`;

    // Source node
    const sourceNode = new models.BoardNode();
    sourceNode.id = generateId();
    sourceNode.remote_name = operation.sourceRemote;
    sourceNode.path = operation.sourcePath;
    sourceNode.label = operation.sourceRemote;
    sourceNode.x = 100;
    sourceNode.y = 100;

    // Target node
    const targetNode = new models.BoardNode();
    targetNode.id = generateId();
    targetNode.remote_name = operation.targetRemote;
    targetNode.path = operation.targetPath;
    targetNode.label = operation.targetRemote;
    targetNode.x = 400;
    targetNode.y = 100;

    board.nodes = [sourceNode, targetNode];

    // Edge
    const edge = new models.BoardEdge();
    edge.id = generateId();
    edge.source_id = sourceNode.id;
    edge.target_id = targetNode.id;
    edge.action = operation.syncConfig.action;

    const profile = new models.Profile();
    profile.parallel = operation.syncConfig.parallel || 8;
    profile.bandwidth = operation.syncConfig.bandwidth
      ? parseInt(operation.syncConfig.bandwidth.replace('M', ''))
      : 0;
    edge.sync_config = profile;

    board.edges = [edge];

    return board;
  }
}
