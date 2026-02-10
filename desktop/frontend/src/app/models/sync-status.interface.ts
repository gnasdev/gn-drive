export interface FileTransferInfo {
  name: string;
  size: number;
  bytes: number;
  progress: number; // 0-100
  status: 'transferring' | 'completed' | 'failed' | 'checking' | 'checked';
  speed?: number; // bytes per second
  error?: string;
}

export interface SyncStatus {
  command: string;
  pid?: number;
  tab_id?: string;
  status: "running" | "completed" | "error" | "stopped";
  progress: number; // 0-100 percentage
  speed: string; // e.g., "1.2 MB/s"
  eta: string; // estimated time remaining
  files_transferred: number;
  total_files: number;
  bytes_transferred: number;
  total_bytes: number;
  current_file: string;
  errors: number;
  checks: number;
  total_checks: number;
  deletes: number;
  renames: number;
  timestamp: string;
  elapsed_time: string;
  action: "pull" | "push" | "bi" | "bi-resync";
  transfers?: FileTransferInfo[];
  delta_mode?: boolean;
  delta_skipped?: boolean;
}

export interface SyncStatusEvent {
  command: string;
  pid?: number;
  tab_id?: string;
  status?: string;
  progress?: number;
  speed?: string;
  eta?: string;
  files_transferred?: number;
  total_files?: number;
  bytes_transferred?: number;
  total_bytes?: number;
  current_file?: string;
  errors?: number;
  checks?: number;
  total_checks?: number;
  deletes?: number;
  renames?: number;
  timestamp?: string;
  elapsed_time?: string;
  action?: string;
  transfers?: FileTransferInfo[];
  delta_mode?: boolean;
  delta_skipped?: boolean;
}

export const DEFAULT_SYNC_STATUS: SyncStatus = {
  command: "sync_status",
  status: "running",
  progress: 0,
  speed: "0 B/s",
  eta: "--",
  files_transferred: 0,
  total_files: 0,
  bytes_transferred: 0,
  total_bytes: 0,
  current_file: "",
  errors: 0,
  checks: 0,
  total_checks: 0,
  deletes: 0,
  renames: 0,
  timestamp: new Date().toISOString(),
  elapsed_time: "0s",
  action: "pull",
};

// Type guards
export function isValidSyncStatus(
  status: string
): status is SyncStatus["status"] {
  return ["running", "completed", "error", "stopped"].includes(status);
}

export function isValidSyncAction(
  action: string
): action is SyncStatus["action"] {
  return ["pull", "push", "bi", "bi-resync"].includes(action);
}
