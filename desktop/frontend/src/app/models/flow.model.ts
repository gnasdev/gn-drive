/**
 * Flow-based Operations Model
 *
 * Core concept:
 * - Flows run in parallel
 * - Operations within a flow run sequentially
 */

export interface SyncConfig {
  action: SyncAction;

  // Performance
  parallel?: number;
  bandwidth?: string; // e.g., "10M"

  // Filters
  includedPaths?: string[];
  excludedPaths?: string[];

  // Conflict resolution (for bisync)
  conflictResolution?: 'newer' | 'older' | 'larger' | 'smaller' | 'path1' | 'path2';

  // Safety
  dryRun?: boolean;
}

export type SyncAction = 'push' | 'pull' | 'bi' | 'bi-resync';

export type OperationStatus = 'idle' | 'pending' | 'running' | 'completed' | 'failed' | 'cancelled';

/**
 * Single sync operation between two remotes
 */
export interface Operation {
  id: string;
  sourceRemote: string;
  sourcePath: string;
  targetRemote: string;
  targetPath: string;

  syncConfig: SyncConfig;

  // Runtime state (not persisted)
  status: OperationStatus;
  logs: string[];

  // UI state
  isExpanded: boolean;
}

/**
 * A flow contains sequential operations that run one after another
 */
export interface Flow {
  id: string;
  name?: string; // Optional label e.g. "Backup to Cloud"
  operations: Operation[]; // Sequential operations
  isCollapsed: boolean; // UI state - flow card collapsed

  // Schedule (applies to entire flow)
  scheduleEnabled: boolean;
  cronExpr?: string;
  lastRun?: Date;
  nextRun?: Date;

  // Runtime state
  status: FlowStatus;
}

export type FlowStatus = 'idle' | 'running' | 'completed' | 'failed' | 'cancelled';

/**
 * Global state for flows management
 */
export interface FlowsState {
  flows: Flow[];
  isDragging: boolean;
  dragData: DragData | null;
}

/**
 * Data transferred during drag-drop
 */
export interface DragData {
  sourceFlowId: string;
  startOperationIndex: number;
  operationCount: number; // How many operations are being dragged
}

// ============ Helper Functions ============

/**
 * Generate a unique ID
 */
export function generateId(): string {
  return `${Date.now()}-${Math.random().toString(36).substring(2, 9)}`;
}

/**
 * Create a new empty flow
 */
export function createEmptyFlow(): Flow {
  return {
    id: `flow-${generateId()}`,
    operations: [],
    isCollapsed: false,
    scheduleEnabled: false,
    status: 'idle',
  };
}

/**
 * Create a new empty operation
 */
export function createEmptyOperation(): Operation {
  return {
    id: `op-${generateId()}`,
    sourceRemote: '',
    sourcePath: '/',
    targetRemote: '',
    targetPath: '/',
    syncConfig: {
      action: 'push',
    },
    status: 'idle',
    logs: [],
    isExpanded: false,
  };
}

/**
 * Maximum log lines per operation
 */
export const MAX_LOG_LINES = 500;

/**
 * Append log to operation, maintaining max buffer size
 */
export function appendOperationLog(operation: Operation, log: string): void {
  operation.logs.push(log);
  if (operation.logs.length > MAX_LOG_LINES) {
    operation.logs = operation.logs.slice(-MAX_LOG_LINES);
  }
}
