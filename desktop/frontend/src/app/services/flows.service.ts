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
  GetBoardExecutionStatus,
  GetBoards,
  GetExecutionLogs,
  StopBoardExecution,
} from '../../../wailsjs/desktop/backend/services/boardservice.js';
import {
  GetFlows,
  SaveFlows,
} from '../../../wailsjs/desktop/backend/services/flowservice.js';
import { isBoardEvent, isSyncEvent, parseEvent, type BoardEvent, type SyncEvent } from '../models/events.js';
import { ErrorService } from './error.service.js';
import {
  createEmptyFlow,
  createEmptyOperation,
  DragData,
  Flow,
  FlowsState,
  generateId,
  MAX_LOG_LINES,
  Operation,
  type SyncAction,
  type SyncConfig,
} from '../models/flow.model.js';

@Injectable({
  providedIn: 'root',
})
export class FlowsService implements OnDestroy {
  private readonly errorService = inject(ErrorService);
  private readonly ngZone = inject(NgZone);

  // State
  readonly state$ = new BehaviorSubject<FlowsState>({
    flows: [],
    isDragging: false,
    dragData: null,
  });

  // Active execution tracking
  private executingFlowId: string | null = null;
  private executingOperationIndex = -1;
  private tempBoardId: string | null = null;

  private eventCleanup: (() => void) | undefined;
  private logPollingInterval: ReturnType<typeof setInterval> | null = null;
  private autoSaveSubscription: Subscription | null = null;
  private autoSaveTrigger$ = new Subject<void>();

  // Board completion tracking (event-driven instead of polling)
  private boardCompletionResolve: ((status: string) => void) | null = null;
  private boardCompletionBoardId: string | null = null;

  constructor() {
    this.eventCleanup = Events.On('tofe', (event) => {
      const rawData = event.data;
      const parsedEvent = parseEvent(rawData);
      if (parsedEvent && isSyncEvent(parsedEvent)) {
        this.ngZone.run(() => this.handleSyncLogEvent(parsedEvent));
      }
      if (parsedEvent && isBoardEvent(parsedEvent)) {
        this.ngZone.run(() => this.handleBoardCompletionEvent(parsedEvent));
      }
    });

    // Auto-save on changes (debounced)
    this.autoSaveSubscription = this.autoSaveTrigger$.pipe(debounceTime(500)).subscribe(() => {
      this.persistFlows();
    });
  }

  ngOnDestroy(): void {
    this.eventCleanup?.();
    this.stopLogPolling();
    this.autoSaveSubscription?.unsubscribe();
  }

  // ============ State Accessors ============

  get flows(): Flow[] {
    return this.state$.value.flows;
  }

  get isDragging(): boolean {
    return this.state$.value.isDragging;
  }

  get dragData(): DragData | null {
    return this.state$.value.dragData;
  }

  // ============ Flow CRUD ============

  /**
   * Load flows from backend (SQLite)
   */
  async loadFlows(): Promise<void> {
    try {
      // Clean up stale temp boards from previous runs
      await this.cleanupStaleTempBoards();

      const backendFlows = await GetFlows();
      const flows = (backendFlows || []).map((bf) => this.toFrontendFlow(bf));
      this.updateState({ flows });
    } catch (err) {
      this.errorService.handleApiError(err, 'Failed to load flows');
    }
  }

  /**
   * Add a new empty flow
   */
  addFlow(): Flow {
    const flow = createEmptyFlow();
    const flows = [...this.flows, flow];
    this.updateState({ flows });
    this.triggerAutoSave();
    return flow;
  }

  /**
   * Remove a flow
   */
  removeFlow(flowId: string): void {
    const flows = this.flows.filter((f) => f.id !== flowId);
    this.updateState({ flows });
    this.triggerAutoSave();
  }

  /**
   * Update flow properties
   */
  updateFlow(flowId: string, updates: Partial<Flow>): void {
    const flows = this.flows.map((f) => (f.id === flowId ? { ...f, ...updates } : f));
    this.updateState({ flows });
    this.triggerAutoSave();
  }

  /**
   * Toggle flow collapsed state
   */
  toggleFlowCollapsed(flowId: string): void {
    const flow = this.flows.find((f) => f.id === flowId);
    if (flow) {
      this.updateFlow(flowId, { isCollapsed: !flow.isCollapsed });
    }
  }

  // ============ Operation CRUD ============

  /**
   * Add a new operation to a flow
   */
  addOperation(flowId: string): Operation {
    const operation = createEmptyOperation();
    const flows = this.flows.map((f) => {
      if (f.id === flowId) {
        return { ...f, operations: [...f.operations, operation] };
      }
      return f;
    });
    this.updateState({ flows });
    this.triggerAutoSave();
    return operation;
  }

  /**
   * Remove an operation from a flow
   */
  removeOperation(flowId: string, operationId: string): void {
    const flows = this.flows.map((f) => {
      if (f.id === flowId) {
        return {
          ...f,
          operations: f.operations.filter((op) => op.id !== operationId),
        };
      }
      return f;
    });
    this.updateState({ flows });
    this.triggerAutoSave();
  }

  /**
   * Update an operation
   */
  updateOperation(flowId: string, operationId: string, updates: Partial<Operation>): void {
    const flows = this.flows.map((f) => {
      if (f.id === flowId) {
        return {
          ...f,
          operations: f.operations.map((op) => (op.id === operationId ? { ...op, ...updates } : op)),
        };
      }
      return f;
    });
    this.updateState({ flows });
    this.triggerAutoSave();
  }

  /**
   * Toggle operation settings expanded state
   */
  toggleOperationExpanded(flowId: string, operationId: string): void {
    const flow = this.flows.find((f) => f.id === flowId);
    const operation = flow?.operations.find((op) => op.id === operationId);
    if (operation) {
      this.updateOperation(flowId, operationId, { isExpanded: !operation.isExpanded });
    }
  }

  // ============ Drag-Drop ============

  /**
   * Start dragging a single operation from a flow
   */
  startDrag(sourceFlowId: string, operationIndex: number): void {
    const flow = this.flows.find((f) => f.id === sourceFlowId);
    if (!flow) return;

    this.updateState({
      isDragging: true,
      dragData: {
        sourceFlowId,
        startOperationIndex: operationIndex,
        operationCount: 1, // Only drag single operation
      },
    });
  }

  /**
   * End dragging (cancel or complete)
   */
  endDrag(): void {
    this.updateState({
      isDragging: false,
      dragData: null,
    });
  }

  /**
   * Move a single operation from source flow to target flow at specified index
   */
  moveOperations(targetFlowId: string, targetIndex: number): void {
    const { dragData } = this.state$.value;
    if (!dragData) return;

    const { sourceFlowId, startOperationIndex } = dragData;

    // Can't drop at same position in same flow
    if (sourceFlowId === targetFlowId && targetIndex === startOperationIndex) {
      this.endDrag();
      return;
    }

    const sourceFlow = this.flows.find((f) => f.id === sourceFlowId);
    if (!sourceFlow) {
      this.endDrag();
      return;
    }

    // Extract single operation to move
    const operationToMove = sourceFlow.operations[startOperationIndex];
    if (!operationToMove) {
      this.endDrag();
      return;
    }

    // Remove from source flow
    let flows = this.flows.map((f) => {
      if (f.id === sourceFlowId) {
        const newOps = [...f.operations];
        newOps.splice(startOperationIndex, 1);
        return { ...f, operations: newOps };
      }
      return f;
    });

    // Insert into target flow
    if (sourceFlowId === targetFlowId) {
      // Moving within same flow - adjust target index if needed
      const adjustedIndex = targetIndex > startOperationIndex ? targetIndex - 1 : targetIndex;
      flows = flows.map((f) => {
        if (f.id === targetFlowId) {
          const newOps = [...f.operations];
          newOps.splice(adjustedIndex, 0, operationToMove);
          return { ...f, operations: newOps };
        }
        return f;
      });
    } else {
      // Moving to different flow
      flows = flows.map((f) => {
        if (f.id === targetFlowId) {
          const newOps = [...f.operations];
          newOps.splice(targetIndex, 0, operationToMove);
          return { ...f, operations: newOps };
        }
        return f;
      });
    }

    this.updateState({ flows, isDragging: false, dragData: null });
    this.triggerAutoSave();
  }

  /**
   * Move a single operation to a new flow
   */
  moveOperationsToNewFlow(): void {
    const { dragData } = this.state$.value;
    if (!dragData) return;

    const { sourceFlowId, startOperationIndex } = dragData;
    const sourceFlow = this.flows.find((f) => f.id === sourceFlowId);
    if (!sourceFlow) {
      this.endDrag();
      return;
    }

    // Extract single operation to move
    const operationToMove = sourceFlow.operations[startOperationIndex];
    if (!operationToMove) {
      this.endDrag();
      return;
    }

    // Create new flow with moved operation
    const newFlow = createEmptyFlow();
    newFlow.operations = [operationToMove];

    // Remove from source flow
    const flows = this.flows.map((f) => {
      if (f.id === sourceFlowId) {
        const newOps = [...f.operations];
        newOps.splice(startOperationIndex, 1);
        return { ...f, operations: newOps };
      }
      return f;
    });

    flows.push(newFlow);

    this.updateState({ flows, isDragging: false, dragData: null });
    this.triggerAutoSave();
  }

  // ============ Execution ============

  /**
   * Execute all operations in a flow sequentially
   */
  async executeFlow(flowId: string): Promise<void> {
    const flow = this.flows.find((f) => f.id === flowId);
    if (!flow || flow.operations.length === 0) return;

    // Check all operations have valid remotes
    const hasInvalidOps = flow.operations.some((op) => !op.sourceRemote || !op.targetRemote);
    if (hasInvalidOps) {
      this.errorService.handleApiError(
        new Error('Some operations have invalid remotes'),
        'Cannot execute flow with incomplete operations'
      );
      return;
    }

    this.executingFlowId = flowId;
    this.executingOperationIndex = 0;
    this.updateFlow(flowId, { status: 'running' });

    // Clear all operation logs and set to pending
    const clearedOps: Operation[] = flow.operations.map((op, idx) => ({
      ...op,
      logs: [] as string[],
      status: (idx === 0 ? 'running' : 'pending') as Operation['status'],
    }));
    this.updateFlowOperations(flowId, clearedOps);

    try {
      // Execute each operation sequentially
      for (let i = 0; i < flow.operations.length; i++) {
        // Check if flow was stopped/cancelled
        if (this.executingFlowId !== flowId) break;

        this.executingOperationIndex = i;
        const operation = this.getFlow(flowId)?.operations[i];
        if (!operation) break;

        // Mark current operation as running
        this.updateOperation(flowId, operation.id, { status: 'running' });

        const boardStatus = await this.executeSingleOperation(flowId, operation);

        // Check if flow was stopped during execution
        if (this.executingFlowId !== flowId) break;

        // Use board status from backend as source of truth
        if (boardStatus === 'failed') {
          this.updateOperation(flowId, operation.id, { status: 'failed' });
          this.updateFlow(flowId, { status: 'failed' });
          break;
        } else if (boardStatus === 'cancelled') {
          this.updateOperation(flowId, operation.id, { status: 'cancelled' });
          this.updateFlow(flowId, { status: 'cancelled' });
          break;
        } else {
          this.updateOperation(flowId, operation.id, { status: 'completed' });
        }
      }

      // Mark flow as completed if all operations succeeded
      const finalFlow = this.getFlow(flowId);
      if (finalFlow?.status === 'running') {
        this.updateFlow(flowId, { status: 'completed' });
      }
    } catch (err) {
      this.updateFlow(flowId, { status: 'failed' });
      this.errorService.handleApiError(err, 'Flow execution failed');
    } finally {
      this.executingFlowId = null;
      this.executingOperationIndex = -1;
      this.stopLogPolling();
    }
  }

  /**
   * Stop executing a flow
   */
  async stopFlow(flowId: string): Promise<void> {
    if (this.tempBoardId) {
      try {
        await StopBoardExecution(this.tempBoardId);
      } catch (err) {
        this.errorService.handleApiError(err, 'Failed to stop execution');
      }
    }

    // Mark flow and current operation as cancelled
    const flow = this.getFlow(flowId);
    if (flow) {
      const ops = flow.operations.map((op, idx) => ({
        ...op,
        status:
          idx === this.executingOperationIndex
            ? 'cancelled'
            : idx < this.executingOperationIndex
              ? op.status
              : ('idle' as const),
      }));
      this.updateFlowOperations(flowId, ops as Operation[]);
      this.updateFlow(flowId, { status: 'cancelled' });
    }

    this.stopLogPolling();
    this.executingFlowId = null;
    this.executingOperationIndex = -1;
    await this.cleanupTempBoard();
  }

  // ============ Private Methods ============

  private getFlow(flowId: string): Flow | undefined {
    return this.flows.find((f) => f.id === flowId);
  }

  private updateFlowOperations(flowId: string, operations: Operation[]): void {
    const flows = this.flows.map((f) => (f.id === flowId ? { ...f, operations } : f));
    this.updateState({ flows });
  }

  private updateState(partial: Partial<FlowsState>): void {
    this.state$.next({ ...this.state$.value, ...partial });
  }

  private triggerAutoSave(): void {
    this.autoSaveTrigger$.next();
  }

  private async executeSingleOperation(flowId: string, operation: Operation): Promise<string> {
    // Create a temporary board for this single operation
    const board = this.operationToBoard(operation);
    this.tempBoardId = board.id;

    try {
      await AddBoard(board);
      await ClearExecutionLogs();
      this.startLogPolling(flowId, operation.id);
      await ExecuteBoard(board.id);

      // Wait for board execution to actually complete (ExecuteBoard returns immediately)
      return await this.waitForBoardCompletion(board.id);
    } finally {
      // Final flush: collect any remaining logs before stopping
      await this.flushRemainingLogs(flowId, operation.id);
      this.stopLogPolling();
      // Clean up this operation's temp board immediately
      await this.cleanupTempBoard();
    }
  }

  private waitForBoardCompletion(boardId: string): Promise<string> {
    return new Promise((resolve) => {
      let resolved = false;
      let fallbackInterval: ReturnType<typeof setInterval> | null = null;

      const doResolve = (status: string) => {
        if (resolved) return;
        resolved = true;
        if (fallbackInterval) clearInterval(fallbackInterval);
        this.boardCompletionResolve = null;
        this.boardCompletionBoardId = null;
        resolve(status);
      };

      this.boardCompletionBoardId = boardId;
      this.boardCompletionResolve = doResolve;

      // Fallback: poll GetBoardExecutionStatus in case the event is missed
      fallbackInterval = setInterval(async () => {
        if (resolved) {
          clearInterval(fallbackInterval!);
          return;
        }
        try {
          const status = await GetBoardExecutionStatus(boardId);
          if (status && status.status !== 'running') {
            doResolve(status.status);
          }
        } catch {
          // Board removed from activeFlows — wait for the event to arrive
        }
      }, 2000);
    });
  }

  private handleBoardCompletionEvent(event: BoardEvent): void {
    if (!this.boardCompletionBoardId || event.boardId !== this.boardCompletionBoardId) return;

    switch (event.type) {
      case 'board:execution:completed':
        this.boardCompletionResolve?.('completed');
        break;
      case 'board:execution:failed':
        this.boardCompletionResolve?.('failed');
        break;
      case 'board:execution:cancelled':
        this.boardCompletionResolve?.('cancelled');
        break;
    }
  }

  private operationToBoard(operation: Operation): models.Board {
    const board = new models.Board();
    board.id = `board-${generateId()}`;
    board.name = `__temp_${generateId()}__`;

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
    profile.bandwidth = operation.syncConfig.bandwidth ? parseInt(operation.syncConfig.bandwidth.replace('M', '')) : 0;
    edge.sync_config = profile;

    board.edges = [edge];

    return board;
  }

  private async cleanupTempBoard(): Promise<void> {
    if (this.tempBoardId) {
      try {
        await DeleteBoard(this.tempBoardId);
      } catch {
        // Ignore cleanup errors
      }
      this.tempBoardId = null;
    }
  }

  private async cleanupStaleTempBoards(): Promise<void> {
    try {
      const boards = await GetBoards();
      for (const board of boards || []) {
        if (board.name?.startsWith('__temp_')) {
          try {
            await DeleteBoard(board.id);
          } catch {
            // Ignore individual cleanup errors
          }
        }
      }
    } catch {
      // Ignore errors during stale board cleanup
    }
  }

  private startLogPolling(flowId: string, operationId: string): void {
    if (this.logPollingInterval) return;

    this.logPollingInterval = setInterval(async () => {
      try {
        const logs = await GetExecutionLogs();
        if (logs && logs.length > 0) {
          this.ngZone.run(() => {
            const flow = this.getFlow(flowId);
            const operation = flow?.operations.find((op) => op.id === operationId);
            if (operation) {
              let newLogs = [...operation.logs, ...logs];
              if (newLogs.length > MAX_LOG_LINES) {
                newLogs = newLogs.slice(-MAX_LOG_LINES);
              }
              // Immutable update: create new operation and flow references
              this.updateFlowOperations(flowId,
                flow!.operations.map((op) =>
                  op.id === operationId ? { ...op, logs: newLogs } : op
                )
              );
            }
          });
        }
      } catch (err) {
        console.error('[FlowsService] Error polling logs:', err);
      }
    }, 500);
  }

  private async flushRemainingLogs(flowId: string, operationId: string): Promise<void> {
    try {
      const logs = await GetExecutionLogs();
      if (logs && logs.length > 0) {
        const flow = this.getFlow(flowId);
        const operation = flow?.operations.find((op) => op.id === operationId);
        if (operation) {
          let newLogs = [...operation.logs, ...logs];
          if (newLogs.length > MAX_LOG_LINES) {
            newLogs = newLogs.slice(-MAX_LOG_LINES);
          }
          this.updateFlowOperations(flowId,
            flow!.operations.map((op) =>
              op.id === operationId ? { ...op, logs: newLogs } : op
            )
          );
        }
      }
    } catch {
      // Ignore errors during final flush
    }
  }

  private stopLogPolling(): void {
    if (this.logPollingInterval) {
      clearInterval(this.logPollingInterval);
      this.logPollingInterval = null;
    }
  }

  private handleSyncLogEvent(event: SyncEvent): void {
    if (!this.executingFlowId) return;

    const flow = this.getFlow(this.executingFlowId);
    const operation = flow?.operations[this.executingOperationIndex];
    if (!operation) return;

    // Append log message with immutable update
    const message = event.message || '';
    if (message) {
      let newLogs = [...operation.logs, message];
      if (newLogs.length > MAX_LOG_LINES) {
        newLogs = newLogs.slice(-MAX_LOG_LINES);
      }
      this.updateFlowOperations(this.executingFlowId,
        flow!.operations.map((op, idx) =>
          idx === this.executingOperationIndex ? { ...op, logs: newLogs } : op
        )
      );
    }

    // Check for completion events
    if (event.type === 'sync:completed') {
      this.updateOperation(this.executingFlowId, operation.id, { status: 'completed' });
    } else if (event.type === 'sync:failed') {
      this.updateOperation(this.executingFlowId, operation.id, { status: 'failed' });
    } else if (event.type === 'sync:cancelled') {
      this.updateOperation(this.executingFlowId, operation.id, { status: 'cancelled' });
    }
  }

  // ============ Persistence (SQLite via FlowService) ============

  private async persistFlows(): Promise<void> {
    try {
      const backendFlows = this.flows.map((f, i) => this.toBackendFlow(f, i));
      await SaveFlows(backendFlows);
    } catch (err) {
      this.errorService.handleApiError(err, 'Failed to save flows');
    }
  }

  // ============ Model Conversion ============

  private toFrontendFlow(bf: models.Flow): Flow {
    return {
      id: bf.id,
      name: bf.name || undefined,
      isCollapsed: bf.is_collapsed,
      scheduleEnabled: bf.schedule_enabled,
      cronExpr: bf.cron_expr || undefined,
      status: 'idle',
      operations: (bf.operations || []).map((bo) => ({
        id: bo.id,
        sourceRemote: bo.source_remote || '',
        sourcePath: bo.source_path || '/',
        targetRemote: bo.target_remote || '',
        targetPath: bo.target_path || '/',
        syncConfig: {
          action: (bo.action || 'push') as SyncAction,
          parallel: bo.parallel || undefined,
          bandwidth: bo.bandwidth || undefined,
          includedPaths: bo.included_paths || undefined,
          excludedPaths: bo.excluded_paths || undefined,
          conflictResolution: (bo.conflict_resolution || undefined) as SyncConfig['conflictResolution'],
          dryRun: bo.dry_run || undefined,
        },
        status: 'idle' as const,
        logs: [] as string[],
        isExpanded: bo.is_expanded || false,
      })),
    };
  }

  private toBackendFlow(f: Flow, sortOrder: number): models.Flow {
    const bf = new models.Flow();
    bf.id = f.id;
    bf.name = f.name || '';
    bf.is_collapsed = f.isCollapsed;
    bf.schedule_enabled = f.scheduleEnabled;
    bf.cron_expr = f.cronExpr || '';
    bf.sort_order = sortOrder;
    bf.operations = f.operations.map((op, opIdx) => {
      const bo = new models.Operation();
      bo.id = op.id;
      bo.flow_id = f.id;
      bo.source_remote = op.sourceRemote;
      bo.source_path = op.sourcePath;
      bo.target_remote = op.targetRemote;
      bo.target_path = op.targetPath;
      bo.action = op.syncConfig.action;
      bo.parallel = op.syncConfig.parallel || 0;
      bo.bandwidth = op.syncConfig.bandwidth || '';
      bo.included_paths = op.syncConfig.includedPaths || [];
      bo.excluded_paths = op.syncConfig.excludedPaths || [];
      bo.conflict_resolution = op.syncConfig.conflictResolution || '';
      bo.dry_run = op.syncConfig.dryRun || false;
      bo.is_expanded = op.isExpanded;
      bo.sort_order = opIdx;
      return bo;
    });
    return bf;
  }
}
