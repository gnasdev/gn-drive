import * as models from "../../../wailsjs/desktop/backend/models/models.js";

// Re-export generated types for convenience
export type Board = models.Board;
export type BoardNode = models.BoardNode;
export type BoardEdge = models.BoardEdge;
export type BoardExecutionStatus = models.BoardExecutionStatus;
export type EdgeExecutionStatus = models.EdgeExecutionStatus;

// Edge execution status colors for SVG rendering
export const EDGE_STATUS_COLORS: Record<string, string> = {
    pending: "#6b7280", // gray-500
    running: "#3b82f6", // blue-500
    completed: "#22c55e", // green-500
    failed: "#ef4444", // red-500
    skipped: "#f97316", // orange-500
};

// Node status colors (derived from connected edges)
export const NODE_STATUS_COLORS: Record<string, string> = {
    idle: "#374151", // gray-700
    pending: "#6b7280", // gray-500
    running: "#3b82f6", // blue-500
    completed: "#22c55e", // green-500
    failed: "#ef4444", // red-500
    skipped: "#f97316", // orange-500
};

// Node dimensions for SVG rendering
export const NODE_WIDTH = 160;
export const NODE_HEIGHT = 64;
export const NODE_HANDLE_RADIUS = 6;

// Action options for edge config
export const EDGE_ACTIONS = [
    { value: "push", label: "Push", icon: "pi pi-upload" },
    { value: "pull", label: "Pull", icon: "pi pi-download" },
    { value: "bi", label: "Bi-Sync", icon: "pi pi-sync" },
    { value: "bi-resync", label: "Resync", icon: "pi pi-replay" },
] as const;

// Generate unique ID
export function generateId(prefix: string): string {
    return `${prefix}-${Date.now()}-${Math.random().toString(36).substring(2, 9)}`;
}

// Calculate edge path between two nodes (straight line with arrowhead offset)
export function calculateEdgePath(
    source: BoardNode,
    target: BoardNode,
    isBidirectional = false,
): string {
    const sx = source.x + NODE_WIDTH / 2;
    const sy = source.y + NODE_HEIGHT / 2;
    const tx = target.x + NODE_WIDTH / 2;
    const ty = target.y + NODE_HEIGHT / 2;

    // Calculate angle to offset the endpoint to the node border
    const dx = tx - sx;
    const dy = ty - sy;
    const dist = Math.sqrt(dx * dx + dy * dy);

    if (dist === 0) return `M ${sx} ${sy} L ${tx} ${ty}`;

    // Offset start and end to node borders
    // Add extra offset for arrowheads (10 pixels for marker)
    const arrowOffset = isBidirectional ? 10 : 0;
    const startX = sx + (dx / dist) * (NODE_WIDTH / 2 + arrowOffset);
    const startY = sy + (dy / dist) * (NODE_HEIGHT / 2 + arrowOffset);
    const endX = tx - (dx / dist) * (NODE_WIDTH / 2 + 10); // always offset end for arrow
    const endY = ty - (dy / dist) * (NODE_HEIGHT / 2 + 10);

    return `M ${startX} ${startY} L ${endX} ${endY}`;
}

// Get edge midpoint for label placement
export function getEdgeMidpoint(
    source: BoardNode,
    target: BoardNode,
): { x: number; y: number } {
    return {
        x: (source.x + target.x + NODE_WIDTH) / 2,
        y: (source.y + target.y + NODE_HEIGHT) / 2,
    };
}
