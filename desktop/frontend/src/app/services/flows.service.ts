import { inject, Injectable, OnDestroy } from '@angular/core';
import { Events } from '@wailsio/runtime';
import { BehaviorSubject, Subject, Subscription } from 'rxjs';
import { debounceTime } from 'rxjs/operators';
import * as models from '../../../wailsjs/desktop/backend/models/models.js';
import {
  AddBoard,
  DeleteBoard,
  ExecuteBoard,
  GetBoardExecutionStatus,
  GetBoards,
  StopBoardExecution,
} from '../../../wailsjs/desktop/backend/services/boardservice.js';
import {
  GetFlows,
  SaveFlows,
} from '../../../wailsjs/desktop/backend/services/flowservice.js';
import { isBoardEvent, isSyncEvent, isSyncStatusDTO, parseEvent, type BoardEvent } from '../models/events.js';
import { type SyncStatus, type SyncStatusEvent } from '../models/sync-status.interface.js';
import { ErrorService } from './error.service.js';
import {
  createEmptyFlow,
  createEmptyOperation,
  DragData,
  Flow,
  FlowsState,
  generateId,
  Operation,
  type SyncAction,
  type SyncConfig,
} from '../models/flow.model.js';

@Injectable({
  providedIn: 'root',
})
export class FlowsService implements OnDestroy {
  private readonly errorService = inject(ErrorService);
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
  private trayEventCleanup: (() => void) | undefined;
  private autoSaveSubscription: Subscription | null = null;
  private autoSaveTrigger$ = new Subject<void>();

  // Board completion tracking (event-driven instead of polling)
  private boardCompletionResolve: ((status: string) => void) | null = null;
  private boardCompletionBoardId: string | null = null;

  constructor() {
    this.eventCleanup = Events.On('tofe', (event) => {
      const rawData = event.data;
      const parsedEvent = parseEvent(rawData);
      if (parsedEvent && isSyncStatusDTO(parsedEvent)) {
        this.handleSyncStatusEvent(parsedEvent as unknown as SyncStatusEvent);
      } else if (parsedEvent && isSyncEvent(parsedEvent)) {
        this.handleSyncLogEvent();
      }
      if (parsedEvent && isBoardEvent(parsedEvent)) {
        this.handleBoardCompletionEvent(parsedEvent);
      }
    });

    // Listen for tray-initiated flow execution
    this.trayEventCleanup = Events.On('tray:execute_flow', (event) => {
      const flowId = event.data as string;
      if (flowId) {
        this.executeFlow(flowId);
      }
    });

    // Auto-save on changes (debounced)
    this.autoSaveSubscription = this.autoSaveTrigger$.pipe(debounceTime(500)).subscribe(() => {
      this.persistFlows();
    });
  }

  ngOnDestroy(): void {
    this.eventCleanup?.();
    this.trayEventCleanup?.();
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
    console.log(`[FlowsService] executeFlow called: flowId=${flowId}`);

    const flow = this.flows.find((f) => f.id === flowId);
    if (!flow || flow.operations.length === 0) {
      console.warn(`[FlowsService] executeFlow: flow not found or empty, flowId=${flowId}, found=${!!flow}, ops=${flow?.operations.length}`);
      return;
    }

    console.log(`[FlowsService] executeFlow: ${flow.operations.length} operations`);
    flow.operations.forEach((op, i) => {
      console.log(`[FlowsService]   op[${i}]: source=${op.sourceRemote}:${op.sourcePath} target=${op.targetRemote}:${op.targetPath} action=${op.syncConfig.action}`);
    });

    // Check all operations have valid remotes
    const hasInvalidOps = flow.operations.some((op) => !op.sourceRemote || !op.targetRemote);
    if (hasInvalidOps) {
      console.error(`[FlowsService] executeFlow: invalid operations (missing remotes)`);
      this.errorService.handleApiError(
        new Error('Some operations have invalid remotes'),
        'Cannot execute flow with incomplete operations'
      );
      return;
    }

    this.executingFlowId = flowId;
    this.executingOperationIndex = 0;
    this.updateFlow(flowId, { status: 'running' });

    // Clear sync status and set to pending
    const clearedOps: Operation[] = flow.operations.map((op, idx) => ({
      ...op,
      syncStatus: undefined,
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
        if (!operation) {
          console.warn(`[FlowsService] executeFlow: operation[${i}] not found after state update`);
          break;
        }

        console.log(`[FlowsService] executeFlow: starting operation[${i}] id=${operation.id}`);

        // Mark current operation as running
        this.updateOperation(flowId, operation.id, { status: 'running' });

        const boardStatus = await this.executeSingleOperation(flowId, operation);

        console.log(`[FlowsService] executeFlow: operation[${i}] result=${boardStatus}`);

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
      console.error(`[FlowsService] executeFlow error:`, err);
      this.updateFlow(flowId, { status: 'failed' });
      this.errorService.handleApiError(err, 'Flow execution failed');
    } finally {
      this.executingFlowId = null;
      this.executingOperationIndex = -1;
    }
  }

  /**
   * Stop executing a flow
   */
  async stopFlow(flowId: string): Promise<void> {
    // Immediately resolve any pending board completion wait so executeFlow unblocks
    if (this.boardCompletionResolve) {
      this.boardCompletionResolve('cancelled');
    }

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

      // Mark in-progress transfers as failed in the cancelled operation's syncStatus
      const cancelledOp = flow.operations[this.executingOperationIndex];
      if (cancelledOp?.syncStatus?.transfers) {
        const updatedTransfers = cancelledOp.syncStatus.transfers.map(t =>
          t.status === 'transferring' || t.status === 'checking'
            ? { ...t, status: 'failed' as const, error: 'Cancelled' }
            : t
        );
        ops[this.executingOperationIndex] = {
          ...ops[this.executingOperationIndex],
          syncStatus: { ...cancelledOp.syncStatus, status: 'stopped' as const, transfers: updatedTransfers },
        };
      }

      this.updateFlowOperations(flowId, ops as Operation[]);
      this.updateFlow(flowId, { status: 'cancelled' });
    }

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

    console.log(`[FlowsService] executeSingleOperation: boardId=${board.id} source=${operation.sourceRemote}:${operation.sourcePath} target=${operation.targetRemote}:${operation.targetPath} action=${operation.syncConfig.action}`);

    try {
      console.log(`[FlowsService] Adding board...`, JSON.stringify(board, null, 2));
      await AddBoard(board);
      console.log(`[FlowsService] Board added successfully`);

      console.log(`[FlowsService] Executing board ${board.id}...`);
      await ExecuteBoard(board.id);
      console.log(`[FlowsService] ExecuteBoard returned, waiting for completion...`);

      // Wait for board execution to actually complete (ExecuteBoard returns immediately)
      const result = await this.waitForBoardCompletion(board.id);
      console.log(`[FlowsService] Board completed with status: ${result}`);
      return result;
    } catch (err) {
      console.error(`[FlowsService] executeSingleOperation error:`, err);
      throw err;
    } finally {
      // Clean up this operation's temp board immediately
      await this.cleanupTempBoard();
    }
  }

  private waitForBoardCompletion(boardId: string): Promise<string> {
    return new Promise((resolve) => {
      let resolved = false;
      let fallbackInterval: ReturnType<typeof setInterval> | null = null;
      let consecutiveErrors = 0;

      const doResolve = (status: string) => {
        if (resolved) return;
        resolved = true;
        if (fallbackInterval) clearInterval(fallbackInterval);
        this.boardCompletionResolve = null;
        this.boardCompletionBoardId = null;
        console.log(`[FlowsService] Board ${boardId} completed with status: ${status}`);
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
          consecutiveErrors = 0;
          if (status && status.status !== 'running') {
            doResolve(status.status);
          }
        } catch {
          // Board removed from activeFlows — execution likely completed already
          consecutiveErrors++;
          console.warn(`[FlowsService] GetBoardExecutionStatus failed (attempt ${consecutiveErrors})`);
          if (consecutiveErrors >= 1) {
            console.warn(`[FlowsService] Board ${boardId} presumed completed after polling failure`);
            doResolve('completed');
          }
        }
      }, 1000);
    });
  }

  private handleBoardCompletionEvent(event: BoardEvent): void {
    console.log(`[FlowsService] handleBoardCompletionEvent: type=${event.type} boardId=${event.boardId} waiting=${this.boardCompletionBoardId}`);
    if (!this.boardCompletionBoardId || event.boardId !== this.boardCompletionBoardId) return;

    switch (event.type) {
      case 'board:execution:completed':
        console.log(`[FlowsService] Board execution completed event received`);
        this.boardCompletionResolve?.('completed');
        break;
      case 'board:execution:failed':
        console.log(`[FlowsService] Board execution failed event received`);
        this.boardCompletionResolve?.('failed');
        break;
      case 'board:execution:cancelled':
        console.log(`[FlowsService] Board execution cancelled event received`);
        this.boardCompletionResolve?.('cancelled');
        break;
    }
  }

  private operationToBoard(operation: Operation): models.Board {
    const board = new models.Board();
    board.id = `board-${generateId()}`;
    board.name = `__temp_${generateId()}__`;

    // Set valid dates — Wails v3 can't deserialize null into Go's non-pointer time.Time
    const now = new Date().toISOString();
    board.created_at = now;
    board.updated_at = now;

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
    edge.sync_config = this.syncConfigToProfile(operation.syncConfig);

    board.edges = [edge];

    console.log(`[FlowsService] operationToBoard: board=${board.id} nodes=${board.nodes.length} edges=${board.edges.length} action=${edge.action}`);
    return board;
  }

  private syncConfigToProfile(sc: SyncConfig): models.Profile {
    const p = new models.Profile();
    // Performance (apply UI placeholder defaults for critical fields)
    p.parallel = sc.parallel || 8;
    p.bandwidth = sc.bandwidth || 0;
    if (sc.multiThreadStreams) p.multi_thread_streams = sc.multiThreadStreams;
    if (sc.bufferSize) p.buffer_size = sc.bufferSize;
    if (sc.retries) p.retries = sc.retries;
    if (sc.lowLevelRetries) p.low_level_retries = sc.lowLevelRetries;
    if (sc.maxDuration) p.max_duration = sc.maxDuration;
    if (sc.checkFirst) p.check_first = true;
    if (sc.orderBy) p.order_by = sc.orderBy;
    if (sc.retriesSleep) p.retries_sleep = sc.retriesSleep;
    if (sc.tpsLimit) p.tps_limit = sc.tpsLimit;
    if (sc.connTimeout) p.conn_timeout = sc.connTimeout;
    if (sc.ioTimeout) p.io_timeout = sc.ioTimeout;
    // Filtering
    p.included_paths = sc.includedPaths || [];
    p.excluded_paths = sc.excludedPaths || [];
    if (sc.minSize) p.min_size = sc.minSize;
    if (sc.maxSize) p.max_size = sc.maxSize;
    if (sc.maxAge) p.max_age = sc.maxAge;
    if (sc.minAge) p.min_age = sc.minAge;
    if (sc.maxDepth != null) p.max_depth = sc.maxDepth;
    if (sc.filterFromFile) p.filter_from_file = sc.filterFromFile;
    if (sc.excludeIfPresent) p.exclude_if_present = sc.excludeIfPresent;
    if (sc.useRegex) p.use_regex = true;
    if (sc.deleteExcluded) p.delete_excluded = true;
    // Safety
    if (sc.dryRun) p.dry_run = true;
    if (sc.maxDelete) p.max_delete = sc.maxDelete;
    if (sc.immutable) p.immutable = true;
    if (sc.maxTransfer) p.max_transfer = sc.maxTransfer;
    if (sc.maxDeleteSize) p.max_delete_size = sc.maxDeleteSize;
    if (sc.suffix) p.suffix = sc.suffix;
    if (sc.suffixKeepExtension) p.suffix_keep_extension = true;
    if (sc.backupPath) p.backup_path = sc.backupPath;
    // Comparison
    if (sc.sizeOnly) p.size_only = true;
    if (sc.updateMode) p.update_mode = true;
    if (sc.ignoreExisting) p.ignore_existing = true;
    // Sync-specific
    if (sc.deleteTiming) p.delete_timing = sc.deleteTiming;
    // Bisync
    if (sc.conflictResolution) p.conflict_resolution = sc.conflictResolution;
    if (sc.resilient) p.resilient = true;
    if (sc.maxLock) p.max_lock = sc.maxLock;
    if (sc.checkAccess) p.check_access = true;
    if (sc.conflictLoser) p.conflict_loser = sc.conflictLoser;
    if (sc.conflictSuffix) p.conflict_suffix = sc.conflictSuffix;
    // Encryption
    if (sc.encryptSource) p.encrypt_source = true;
    if (sc.encryptDest) p.encrypt_dest = true;
    if (sc.encryptPassword) p.encrypt_password = sc.encryptPassword;
    if (sc.encryptPassword2) p.encrypt_password2 = sc.encryptPassword2;
    if (sc.encryptFilename) p.encrypt_filename = sc.encryptFilename;
    if (sc.encryptDirectory) p.encrypt_directory = true;
    return p;
  }

  private profileToSyncConfig(p: models.Profile, action: string): SyncConfig {
    const sc: SyncConfig = {
      action: (action || 'push') as SyncAction,
    };
    // Performance
    if (p.parallel) sc.parallel = p.parallel;
    if (p.bandwidth) sc.bandwidth = p.bandwidth;
    if (p.multi_thread_streams) sc.multiThreadStreams = p.multi_thread_streams;
    if (p.buffer_size) sc.bufferSize = p.buffer_size;
    if (p.retries) sc.retries = p.retries;
    if (p.low_level_retries) sc.lowLevelRetries = p.low_level_retries;
    if (p.max_duration) sc.maxDuration = p.max_duration;
    if (p.check_first) sc.checkFirst = true;
    if (p.order_by) sc.orderBy = p.order_by;
    if (p.retries_sleep) sc.retriesSleep = p.retries_sleep;
    if (p.tps_limit) sc.tpsLimit = p.tps_limit;
    if (p.conn_timeout) sc.connTimeout = p.conn_timeout;
    if (p.io_timeout) sc.ioTimeout = p.io_timeout;
    // Filtering
    if (p.included_paths?.length) sc.includedPaths = p.included_paths;
    if (p.excluded_paths?.length) sc.excludedPaths = p.excluded_paths;
    if (p.min_size) sc.minSize = p.min_size;
    if (p.max_size) sc.maxSize = p.max_size;
    if (p.max_age) sc.maxAge = p.max_age;
    if (p.min_age) sc.minAge = p.min_age;
    if (p.max_depth != null) sc.maxDepth = p.max_depth;
    if (p.filter_from_file) sc.filterFromFile = p.filter_from_file;
    if (p.exclude_if_present) sc.excludeIfPresent = p.exclude_if_present;
    if (p.use_regex) sc.useRegex = true;
    if (p.delete_excluded) sc.deleteExcluded = true;
    // Safety
    if (p.dry_run) sc.dryRun = true;
    if (p.max_delete) sc.maxDelete = p.max_delete;
    if (p.immutable) sc.immutable = true;
    if (p.max_transfer) sc.maxTransfer = p.max_transfer;
    if (p.max_delete_size) sc.maxDeleteSize = p.max_delete_size;
    if (p.suffix) sc.suffix = p.suffix;
    if (p.suffix_keep_extension) sc.suffixKeepExtension = true;
    if (p.backup_path) sc.backupPath = p.backup_path;
    // Comparison
    if (p.size_only) sc.sizeOnly = true;
    if (p.update_mode) sc.updateMode = true;
    if (p.ignore_existing) sc.ignoreExisting = true;
    // Sync-specific
    if (p.delete_timing) sc.deleteTiming = p.delete_timing as SyncConfig['deleteTiming'];
    // Bisync
    if (p.conflict_resolution) sc.conflictResolution = p.conflict_resolution as SyncConfig['conflictResolution'];
    if (p.resilient) sc.resilient = true;
    if (p.max_lock) sc.maxLock = p.max_lock;
    if (p.check_access) sc.checkAccess = true;
    if (p.conflict_loser) sc.conflictLoser = p.conflict_loser as SyncConfig['conflictLoser'];
    if (p.conflict_suffix) sc.conflictSuffix = p.conflict_suffix;
    // Encryption
    if (p.encrypt_source) sc.encryptSource = true;
    if (p.encrypt_dest) sc.encryptDest = true;
    if (p.encrypt_password) sc.encryptPassword = p.encrypt_password;
    if (p.encrypt_password2) sc.encryptPassword2 = p.encrypt_password2;
    if (p.encrypt_filename) sc.encryptFilename = p.encrypt_filename as SyncConfig['encryptFilename'];
    if (p.encrypt_directory) sc.encryptDirectory = true;
    return sc;
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

  private handleSyncLogEvent(): void {
    // Operation status is managed by executeFlow via board completion events.
    // No need to duplicate status updates here — doing so would prematurely
    // change the operation status and cause the UI to lose the syncStatus display.
  }

  private handleSyncStatusEvent(event: SyncStatusEvent): void {
    if (!this.executingFlowId) return;

    const flow = this.getFlow(this.executingFlowId);
    const operation = flow?.operations[this.executingOperationIndex];
    if (!operation) return;

    // Build SyncStatus from event
    const syncStatus: SyncStatus = {
      command: event.command || 'sync_status',
      pid: event.pid,
      tab_id: event.tab_id,
      status: (['running', 'completed', 'error', 'stopped'].includes(event.status || '') ? event.status : 'running') as SyncStatus['status'],
      progress: event.progress || 0,
      speed: event.speed || '0 B/s',
      eta: event.eta || '--',
      files_transferred: event.files_transferred || 0,
      total_files: event.total_files || 0,
      bytes_transferred: event.bytes_transferred || 0,
      total_bytes: event.total_bytes || 0,
      current_file: event.current_file || '',
      errors: event.errors || 0,
      checks: event.checks || 0,
      total_checks: event.total_checks || 0,
      deletes: event.deletes || 0,
      renames: event.renames || 0,
      timestamp: event.timestamp || new Date().toISOString(),
      elapsed_time: event.elapsed_time || '0s',
      action: (['pull', 'push', 'bi', 'bi-resync'].includes(event.action || '') ? event.action : 'push') as SyncStatus['action'],
      transfers: event.transfers,
    };

    // Immutable update: set syncStatus
    this.updateFlowOperations(this.executingFlowId,
      flow!.operations.map((op, idx) =>
        idx === this.executingOperationIndex
          ? { ...op, syncStatus }
          : op
      )
    );
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
        syncConfig: this.profileToSyncConfig(bo.sync_config || new models.Profile(), bo.action),
        status: 'idle' as const,
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
      bo.sync_config = this.syncConfigToProfile(op.syncConfig);
      bo.is_expanded = op.isExpanded;
      bo.sort_order = opIdx;
      return bo;
    });
    return bf;
  }
}
