import { SyncStatus } from './sync-status.interface';

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
  bandwidth?: number; // MB/s
  multiThreadStreams?: number;
  bufferSize?: string; // e.g., "16M"
  retries?: number;
  lowLevelRetries?: number;
  maxDuration?: string; // e.g., "1h30m"
  checkFirst?: boolean;
  orderBy?: string; // e.g., "size,desc"
  retriesSleep?: string; // e.g., "10s"
  tpsLimit?: number;
  connTimeout?: string; // e.g., "30s"
  ioTimeout?: string; // e.g., "5m"

  // Filtering
  includedPaths?: string[];
  excludedPaths?: string[];
  minSize?: string; // e.g., "100k"
  maxSize?: string; // e.g., "1G"
  maxAge?: string; // e.g., "24h", "7d"
  minAge?: string;
  maxDepth?: number;
  filterFromFile?: string;
  excludeIfPresent?: string; // e.g., ".nosync"
  useRegex?: boolean;
  deleteExcluded?: boolean;

  // Safety
  dryRun?: boolean;
  maxDelete?: number; // percentage 0-100
  immutable?: boolean;
  maxTransfer?: string; // e.g., "10G"
  maxDeleteSize?: string; // e.g., "1G"
  suffix?: string;
  suffixKeepExtension?: boolean;
  backupPath?: string;

  // Comparison
  sizeOnly?: boolean;
  updateMode?: boolean;
  ignoreExisting?: boolean;

  // Sync-specific (push/pull)
  deleteTiming?: 'before' | 'during' | 'after';

  // Bisync
  conflictResolution?: 'newer' | 'older' | 'larger' | 'smaller' | 'path1' | 'path2';
  resilient?: boolean;
  maxLock?: string; // e.g., "15m"
  checkAccess?: boolean;
  conflictLoser?: 'num' | 'pathname' | 'delete';
  conflictSuffix?: string;

  // Encryption (on-the-fly crypt wrapping)
  encryptSource?: boolean;
  encryptDest?: boolean;
  encryptPassword?: string;
  encryptPassword2?: string;
  encryptFilename?: 'standard' | 'obfuscate' | 'off';
  encryptDirectory?: boolean;
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
  syncStatus?: SyncStatus;

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
    isExpanded: false,
  };
}

