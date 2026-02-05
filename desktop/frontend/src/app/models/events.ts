// Event types matching backend events/types.go

export type EventType =
    // Sync Events
    | "sync:started"
    | "sync:progress"
    | "sync:completed"
    | "sync:failed"
    | "sync:cancelled"
    // Config Events
    | "config:updated"
    | "profile:added"
    | "profile:updated"
    | "profile:deleted"
    // Remote Events
    | "remote:added"
    | "remote:updated"
    | "remote:deleted"
    | "remotes:list"
    // Tab Events
    | "tab:created"
    | "tab:updated"
    | "tab:deleted"
    | "tab:output"
    // Error Events
    | "error:occurred"
    // Board Events
    | "board:updated"
    | "board:execution:started"
    | "board:execution:progress"
    | "board:execution:completed"
    | "board:execution:failed"
    | "board:execution:cancelled"
    // Legacy command types
    | "command_started"
    | "command_stoped"
    | "command_output"
    | "error"
    | "sync_status";

// Base event structure
export interface BaseEvent {
    type: EventType;
    timestamp: string;
    data?: unknown;
}

// Sync event structure
export interface SyncEvent extends BaseEvent {
    type:
        | "sync:started"
        | "sync:progress"
        | "sync:completed"
        | "sync:failed"
        | "sync:cancelled";
    tabId?: string;
    action: string;
    progress?: number;
    status: string;
    message?: string;
    seqNo?: number; // Sequence number for reliable log delivery
}

// Config event structure
export interface ConfigEvent extends BaseEvent {
    type:
        | "config:updated"
        | "profile:added"
        | "profile:updated"
        | "profile:deleted";
    profileId?: string;
}

// Remote event structure
export interface RemoteEvent extends BaseEvent {
    type: "remote:added" | "remote:updated" | "remote:deleted" | "remotes:list";
    remoteName?: string;
}

// Tab event structure
export interface TabEvent extends BaseEvent {
    type: "tab:created" | "tab:updated" | "tab:deleted" | "tab:output";
    tabId: string;
    tabName?: string;
}

// Error event structure
export interface ErrorEvent extends BaseEvent {
    type: "error:occurred";
    code: string;
    message: string;
    details?: string;
    tabId?: string;
}

// Board event structure
export interface BoardEvent extends BaseEvent {
    type:
        | "board:updated"
        | "board:execution:started"
        | "board:execution:progress"
        | "board:execution:completed"
        | "board:execution:failed"
        | "board:execution:cancelled";
    boardId: string;
    edgeId?: string;
    status: string;
    message?: string;
}

// Legacy command DTO (for backward compatibility)
export interface CommandDTO {
    command: string;
    pid?: number;
    task?: string;
    error?: string;
    tab_id?: string;
}

// Union type for all possible events
export type AppEvent =
    | SyncEvent
    | ConfigEvent
    | RemoteEvent
    | TabEvent
    | ErrorEvent
    | BoardEvent
    | CommandDTO;

// Type guards
export function isSyncEvent(event: unknown): event is SyncEvent {
    if (typeof event !== "object" || event === null) return false;
    const e = event as Record<string, unknown>;
    return (
        typeof e["type"] === "string" &&
        (e["type"] as string).startsWith("sync:")
    );
}

export function isConfigEvent(event: unknown): event is ConfigEvent {
    if (typeof event !== "object" || event === null) return false;
    const e = event as Record<string, unknown>;
    const type = e["type"] as string;
    return (
        typeof type === "string" &&
        (type.startsWith("config:") || type.startsWith("profile:"))
    );
}

export function isRemoteEvent(event: unknown): event is RemoteEvent {
    if (typeof event !== "object" || event === null) return false;
    const e = event as Record<string, unknown>;
    const type = e["type"] as string;
    return (
        typeof type === "string" &&
        (type.startsWith("remote:") || type === "remotes:list")
    );
}

export function isTabEvent(event: unknown): event is TabEvent {
    if (typeof event !== "object" || event === null) return false;
    const e = event as Record<string, unknown>;
    return (
        typeof e["type"] === "string" &&
        (e["type"] as string).startsWith("tab:")
    );
}

export function isErrorEvent(event: unknown): event is ErrorEvent {
    if (typeof event !== "object" || event === null) return false;
    const e = event as Record<string, unknown>;
    return e["type"] === "error:occurred";
}

export function isBoardEvent(event: unknown): event is BoardEvent {
    if (typeof event !== "object" || event === null) return false;
    const e = event as Record<string, unknown>;
    return (
        typeof e["type"] === "string" &&
        (e["type"] as string).startsWith("board:")
    );
}

export function isLegacyCommandDTO(event: unknown): event is CommandDTO {
    if (typeof event !== "object" || event === null) return false;
    const e = event as Record<string, unknown>;
    return typeof e["command"] === "string" && !("type" in e);
}

// Parse event from raw data (can be JSON string or already parsed object)
export function parseEvent(rawData: unknown): AppEvent | null {
    try {
        // If rawData is already an object, return it directly
        if (typeof rawData === "object" && rawData !== null) {
            return rawData as AppEvent;
        }
        // If rawData is a string, parse it as JSON
        if (typeof rawData === "string") {
            const parsed = JSON.parse(rawData);
            return parsed as AppEvent;
        }
        console.error("Unexpected rawData type:", typeof rawData, rawData);
        return null;
    } catch (e) {
        console.error("Failed to parse event:", rawData, e);
        return null;
    }
}
