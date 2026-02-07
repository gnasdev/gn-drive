/**
 * Operation represents a sync operation between two remotes.
 * Operations can be nested to create sequential or parallel workflows.
 */
export interface Operation {
  id: string;

  // Source and target remotes
  sourceRemote: string; // Remote name or "local"
  sourcePath: string;
  targetRemote: string;
  targetPath: string;

  // Group type for workflow nesting
  groupType: 'single' | 'sequential' | 'parallel';

  // Nested operations (for sequential/parallel groups)
  children?: Operation[];
  parentId?: string;

  // Sync configuration
  syncConfig: SyncConfig;

  // Schedule settings
  scheduleEnabled: boolean;
  cronExpr?: string;
  lastRun?: Date;
  nextRun?: Date;

  // Execution state (runtime, not persisted)
  status: OperationStatus;
  logs: string[]; // Last 500 lines

  // UI state
  isExpanded: boolean; // Settings panel expanded
  isLogsVisible: boolean; // Logs panel visible
}

export interface SyncConfig {
  action: SyncAction;

  // Performance
  parallel?: number;
  bandwidth?: string; // e.g., "10M"
  checkers?: number;

  // Filters
  includedPaths?: string[];
  excludedPaths?: string[];

  // Conflict resolution (for bisync)
  conflictResolution?: 'newer' | 'older' | 'larger' | 'smaller' | 'path1' | 'path2';

  // Safety
  dryRun?: boolean;
  deleteMode?: 'default' | 'before' | 'after' | 'during';

  // Advanced
  compareMode?: 'default' | 'size' | 'modtime' | 'checksum';
  copyLinks?: boolean;
  noTraverse?: boolean;
}

export type SyncAction = 'push' | 'pull' | 'bi' | 'bi-resync';

export type OperationStatus = 'idle' | 'pending' | 'running' | 'completed' | 'failed' | 'cancelled';

/**
 * State for the operations tree
 */
export interface OperationsState {
  operations: Operation[]; // Root level operations (tree structure)
  activeOperationId?: string; // Currently executing operation
  dragState?: DragState;
}

export interface DragState {
  operationId: string;
  dropTarget?: {
    operationId: string;
    position: 'before' | 'after' | 'inside';
  };
}

/**
 * Helper to create a new empty operation
 */
export function createEmptyOperation(): Operation {
  return {
    id: generateId(),
    sourceRemote: '',
    sourcePath: '/',
    targetRemote: '',
    targetPath: '/',
    groupType: 'single',
    syncConfig: {
      action: 'push',
    },
    scheduleEnabled: false,
    status: 'idle',
    logs: [],
    isExpanded: false,
    isLogsVisible: false,
  };
}

/**
 * Generate a unique ID for operations
 */
export function generateId(): string {
  return `op-${Date.now()}-${Math.random().toString(36).substring(2, 9)}`;
}

/**
 * Maximum number of log lines to keep per operation
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

/**
 * Flatten operations tree to array (for iteration)
 */
export function flattenOperations(operations: Operation[]): Operation[] {
  const result: Operation[] = [];
  for (const op of operations) {
    result.push(op);
    if (op.children) {
      result.push(...flattenOperations(op.children));
    }
  }
  return result;
}

/**
 * Find operation by ID in tree
 */
export function findOperationById(operations: Operation[], id: string): Operation | undefined {
  for (const op of operations) {
    if (op.id === id) return op;
    if (op.children) {
      const found = findOperationById(op.children, id);
      if (found) return found;
    }
  }
  return undefined;
}
