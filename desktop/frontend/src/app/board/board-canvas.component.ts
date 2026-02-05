import { CommonModule } from "@angular/common";
import {
    ChangeDetectionStrategy,
    ChangeDetectorRef,
    Component,
    ElementRef,
    EventEmitter,
    HostListener,
    inject,
    Input,
    Output,
    ViewChild,
} from "@angular/core";
import * as models from "../../../wailsjs/desktop/backend/models/models.js";
import {
    calculateEdgePath,
    EDGE_STATUS_COLORS,
    getEdgeMidpoint,
    NODE_HEIGHT,
    NODE_STATUS_COLORS,
    NODE_WIDTH,
} from "./board.types.js";

@Component({
    selector: "app-board-canvas",
    standalone: true,
    imports: [CommonModule],
    changeDetection: ChangeDetectionStrategy.OnPush,
    template: `
        <svg
            #svgCanvas
            class="w-full h-full bg-gray-900"
            [class.cursor-grabbing]="isPanning || draggingNode"
            [class.cursor-crosshair]="connectingFrom"
            [class.cursor-grab]="!isPanning && !draggingNode && !connectingFrom"
            [attr.viewBox]="viewBox"
            (mousedown)="onCanvasMouseDown($event)"
            (mousemove)="onCanvasMouseMove($event)"
            (mouseup)="onCanvasMouseUp($event)"
            (wheel)="onCanvasWheel($event)"
            (dragover)="onDragOver($event)"
            (drop)="onDrop($event)"
        >
            <defs>
                <!-- End markers (arrow pointing forward) -->
                <marker
                    id="arrowhead"
                    markerWidth="10"
                    markerHeight="7"
                    refX="10"
                    refY="3.5"
                    orient="auto"
                >
                    <polygon points="0 0, 10 3.5, 0 7" fill="#9ca3af" />
                </marker>
                <marker
                    id="arrowhead-selected"
                    markerWidth="10"
                    markerHeight="7"
                    refX="10"
                    refY="3.5"
                    orient="auto"
                >
                    <polygon points="0 0, 10 3.5, 0 7" fill="#60a5fa" />
                </marker>
                @for (entry of statusColorEntries; track entry[0]) {
                    <marker
                        [id]="'arrowhead-' + entry[0]"
                        markerWidth="10"
                        markerHeight="7"
                        refX="10"
                        refY="3.5"
                        orient="auto"
                    >
                        <polygon
                            points="0 0, 10 3.5, 0 7"
                            [attr.fill]="entry[1]"
                        />
                    </marker>
                }

                <!-- Start markers (arrow pointing backward, for bi-sync) -->
                <marker
                    id="arrowhead-start"
                    markerWidth="10"
                    markerHeight="7"
                    refX="0"
                    refY="3.5"
                    orient="auto"
                >
                    <polygon points="10 0, 0 3.5, 10 7" fill="#9ca3af" />
                </marker>
                <marker
                    id="arrowhead-start-selected"
                    markerWidth="10"
                    markerHeight="7"
                    refX="0"
                    refY="3.5"
                    orient="auto"
                >
                    <polygon points="10 0, 0 3.5, 10 7" fill="#60a5fa" />
                </marker>
                @for (entry of statusColorEntries; track entry[0]) {
                    <marker
                        [id]="'arrowhead-start-' + entry[0]"
                        markerWidth="10"
                        markerHeight="7"
                        refX="0"
                        refY="3.5"
                        orient="auto"
                    >
                        <polygon
                            points="10 0, 0 3.5, 10 7"
                            [attr.fill]="entry[1]"
                        />
                    </marker>
                }
            </defs>

            <!-- Grid pattern -->
            <defs>
                <pattern
                    id="grid"
                    width="20"
                    height="20"
                    patternUnits="userSpaceOnUse"
                >
                    <path
                        d="M 20 0 L 0 0 0 20"
                        fill="none"
                        stroke="#1f2937"
                        stroke-width="0.5"
                    />
                </pattern>
            </defs>
            <rect
                class="canvas-bg"
                width="5000"
                height="5000"
                x="-2500"
                y="-2500"
                fill="url(#grid)"
            />

            <!-- Edges -->
            @for (edge of edges; track edge.id) {
                @if (getEdgeNodes(edge); as nodes) {
                    <g
                        class="cursor-pointer"
                        (click)="onEdgeClick($event, edge)"
                        (dblclick)="onEdgeDoubleClick($event, edge)"
                        (contextmenu)="onEdgeContextMenu($event, edge)"
                    >
                        <path
                            [attr.d]="
                                getPath(
                                    nodes.source,
                                    nodes.target,
                                    isBidirectional(edge)
                                )
                            "
                            fill="none"
                            [attr.stroke]="getEdgeColor(edge)"
                            [attr.stroke-width]="
                                selectedEdgeId === edge.id ? 3 : 2
                            "
                            [attr.marker-start]="getMarkerStart(edge)"
                            [attr.marker-end]="getMarkerEnd(edge)"
                            stroke-linecap="round"
                        />
                        <!-- Invisible wider path for easier clicking -->
                        <path
                            [attr.d]="
                                getPath(
                                    nodes.source,
                                    nodes.target,
                                    isBidirectional(edge)
                                )
                            "
                            fill="none"
                            stroke="transparent"
                            stroke-width="12"
                        />
                        <!-- Edge label -->
                        <text
                            [attr.x]="getMidpoint(nodes.source, nodes.target).x"
                            [attr.y]="
                                getMidpoint(nodes.source, nodes.target).y - 8
                            "
                            text-anchor="middle"
                            class="text-xs select-none pointer-events-none"
                            fill="#9ca3af"
                            font-size="11"
                        >
                            {{ getActionLabel(edge.action) }}
                        </text>
                        <!-- Draggable endpoint at source (only when edge is selected) -->
                        @if (selectedEdgeId === edge.id && !disabled) {
                            <circle
                                [attr.cx]="nodes.source.x + nodeWidth / 2"
                                [attr.cy]="nodes.source.y + nodeHeight / 2"
                                r="8"
                                fill="#3b82f6"
                                stroke="#1e3a8a"
                                stroke-width="2"
                                class="cursor-grab hover:fill-blue-400"
                                (mousedown)="
                                    onEdgeEndpointMouseDown(
                                        $event,
                                        edge,
                                        'source'
                                    )
                                "
                            />
                            <!-- Draggable endpoint at target -->
                            <circle
                                [attr.cx]="nodes.target.x + nodeWidth / 2"
                                [attr.cy]="nodes.target.y + nodeHeight / 2"
                                r="8"
                                fill="#3b82f6"
                                stroke="#1e3a8a"
                                stroke-width="2"
                                class="cursor-grab hover:fill-blue-400"
                                (mousedown)="
                                    onEdgeEndpointMouseDown(
                                        $event,
                                        edge,
                                        'target'
                                    )
                                "
                            />
                        }
                    </g>
                }
            }

            <!-- Connection line while dragging -->
            @if ((connectingFrom || reconnectingEdge) && connectMousePos) {
                <line
                    [attr.x1]="getConnectionAnchorX()"
                    [attr.y1]="getConnectionAnchorY()"
                    [attr.x2]="
                        hoverTargetNode
                            ? hoverTargetNode.x + nodeWidth / 2
                            : connectMousePos.x
                    "
                    [attr.y2]="
                        hoverTargetNode
                            ? hoverTargetNode.y + nodeHeight / 2
                            : connectMousePos.y
                    "
                    [attr.stroke]="hoverTargetNode ? '#22c55e' : '#60a5fa'"
                    stroke-width="2"
                    [attr.stroke-dasharray]="hoverTargetNode ? '0' : '5,5'"
                />
            }

            <!-- Nodes -->
            @for (node of nodes; track node.id) {
                <g
                    [attr.transform]="
                        'translate(' + node.x + ',' + node.y + ')'
                    "
                    [class.cursor-move]="!disabled"
                    [class.cursor-default]="disabled"
                    (mousedown)="onNodeMouseDown($event, node)"
                    (dblclick)="onNodeDoubleClick($event, node)"
                    (contextmenu)="onNodeContextMenu($event, node)"
                >
                    <!-- Magnet highlight glow for valid connection targets -->
                    @if (isValidConnectionTarget(node)) {
                        <rect
                            [attr.width]="nodeWidth + 8"
                            [attr.height]="nodeHeight + 8"
                            x="-4"
                            y="-4"
                            rx="12"
                            ry="12"
                            fill="none"
                            [attr.stroke]="
                                isHoveredTarget(node) ? '#22c55e' : '#3b82f6'
                            "
                            [attr.stroke-width]="isHoveredTarget(node) ? 3 : 2"
                            [attr.opacity]="isHoveredTarget(node) ? 1 : 0.5"
                            stroke-dasharray="6,3"
                            class="animate-pulse"
                        />
                    }
                    <!-- Node background -->
                    <rect
                        [attr.width]="nodeWidth"
                        [attr.height]="nodeHeight"
                        rx="8"
                        ry="8"
                        [attr.fill]="getNodeFill(node)"
                        [attr.stroke]="
                            isHoveredTarget(node)
                                ? '#22c55e'
                                : getNodeStroke(node)
                        "
                        [attr.stroke-width]="
                            isHoveredTarget(node) ? 3 : getNodeStrokeWidth(node)
                        "
                    />
                    <!-- Running indicator animation -->
                    @if (getNodeStatus(node) === "running") {
                        <rect
                            [attr.width]="nodeWidth"
                            [attr.height]="nodeHeight"
                            rx="8"
                            ry="8"
                            fill="none"
                            stroke="#3b82f6"
                            stroke-width="2"
                            class="animate-pulse"
                            opacity="0.6"
                        />
                    }
                    <!-- Remote name -->
                    <text
                        [attr.x]="nodeWidth / 2"
                        y="24"
                        text-anchor="middle"
                        fill="#e5e7eb"
                        font-size="13"
                        font-weight="600"
                        class="select-none pointer-events-none"
                    >
                        {{ node.label || node.remote_name || "(No name)" }}
                    </text>
                    <!-- Provider & Path subtitle -->
                    <text
                        [attr.x]="nodeWidth / 2"
                        y="44"
                        text-anchor="middle"
                        fill="#6b7280"
                        font-size="10"
                        class="select-none pointer-events-none"
                    >
                        @if (getNodeProviderLabel(node)) {
                            <tspan fill="#9ca3af">
                                {{ getNodeProviderLabel(node) }}
                            </tspan>
                            @if (node.path) {
                                <tspan fill="#6b7280">
                                    · {{ truncatePath(node.path) }}
                                </tspan>
                            }
                        } @else {
                            {{ truncatePath(node.path) || "(No path)" }}
                        }
                    </text>
                    <!-- Connector button (right side) - for creating edges -->
                    @if (!disabled) {
                        <g
                            [attr.transform]="
                                'translate(' +
                                (nodeWidth + 8) +
                                ',' +
                                (nodeHeight / 2 - 10) +
                                ')'
                            "
                            class="cursor-pointer"
                            (mousedown)="onConnectorMouseDown($event, node)"
                        >
                            <rect
                                width="20"
                                height="20"
                                rx="4"
                                fill="#374151"
                                stroke="#4b5563"
                                stroke-width="1"
                                class="hover:fill-gray-600 transition-colors"
                            />
                            <!-- Arrow right icon -->
                            <path
                                d="M6 10h8M11 6l4 4-4 4"
                                fill="none"
                                stroke="#9ca3af"
                                stroke-width="1.5"
                                stroke-linecap="round"
                                stroke-linejoin="round"
                                class="pointer-events-none"
                            />
                        </g>
                    }
                </g>
            }
        </svg>
    `,
})
export class BoardCanvasComponent {
    @ViewChild("svgCanvas") svgCanvas!: ElementRef<SVGSVGElement>;

    @Input() nodes: models.BoardNode[] = [];
    @Input() edges: models.BoardEdge[] = [];
    @Input() selectedNodeId: string | null = null;
    @Input() selectedEdgeId: string | null = null;
    @Input() executionStatus: models.BoardExecutionStatus | null = null;
    @Input() disabled = false;
    @Input() remoteProviders = new Map<string, string>();

    @Output() nodeSelected = new EventEmitter<string | null>();
    @Output() edgeSelected = new EventEmitter<string | null>();
    @Output() nodeMoved = new EventEmitter<{
        nodeId: string;
        x: number;
        y: number;
    }>();
    @Output() edgeCreated = new EventEmitter<{
        sourceId: string;
        targetId: string;
    }>();
    @Output() selectionCleared = new EventEmitter<void>();
    @Output() nodeContextMenu = new EventEmitter<{
        event: MouseEvent;
        node: models.BoardNode;
    }>();
    @Output() nodeDoubleClicked = new EventEmitter<models.BoardNode>();
    @Output() edgeDoubleClicked = new EventEmitter<string>();
    @Output() edgeContextMenu = new EventEmitter<{
        event: MouseEvent;
        edge: models.BoardEdge;
    }>();
    @Output() nodeDeleted = new EventEmitter<string>();
    @Output() edgeDeleted = new EventEmitter<string>();
    @Output() canvasDrop = new EventEmitter<{ x: number; y: number }>();
    @Output() edgeReconnected = new EventEmitter<{
        edgeId: string;
        endpoint: "source" | "target";
        newNodeId: string;
    }>();

    readonly nodeWidth = NODE_WIDTH;
    readonly nodeHeight = NODE_HEIGHT;
    readonly statusColorEntries = Object.entries(EDGE_STATUS_COLORS);

    // Viewport
    viewBox = "-50 -50 900 600";
    private panX = -50;
    private panY = -50;
    private viewWidth = 900;
    private viewHeight = 600;

    // Drag state
    draggingNode: models.BoardNode | null = null;
    private dragOffset = { x: 0, y: 0 };
    isPanning = false;
    private panStart = { x: 0, y: 0 };

    // Connection state
    connectingFrom: models.BoardNode | null = null;
    connectMousePos: { x: number; y: number } | null = null;
    hoverTargetNode: models.BoardNode | null = null;

    // Edge reconnection state
    reconnectingEdge: models.BoardEdge | null = null;
    reconnectEndpoint: "source" | "target" | null = null;
    reconnectOriginalNodeId: string | null = null;

    private readonly cdr = inject(ChangeDetectorRef);

    @HostListener("document:keydown", ["$event"])
    onKeyDown(event: KeyboardEvent): void {
        if (this.disabled) return;
        if (event.key === "Delete" || event.key === "Backspace") {
            // Prevent default browser behavior (e.g., navigating back)
            event.preventDefault();
            if (this.selectedNodeId) {
                this.nodeDeleted.emit(this.selectedNodeId);
            } else if (this.selectedEdgeId) {
                this.edgeDeleted.emit(this.selectedEdgeId);
            }
        }
    }

    onNodeMouseDown(event: MouseEvent, node: models.BoardNode): void {
        event.stopPropagation();
        if (this.disabled) return; // Allow panning but not node dragging
        this.nodeSelected.emit(node.id);

        // Shift + drag to create connection
        if (event.shiftKey) {
            this.connectingFrom = node;
            this.connectMousePos = this.getSvgPoint(event);
            return;
        }

        const svgPoint = this.getSvgPoint(event);
        this.draggingNode = node;
        this.dragOffset = {
            x: svgPoint.x - node.x,
            y: svgPoint.y - node.y,
        };
    }

    onNodeDoubleClick(event: MouseEvent, node: models.BoardNode): void {
        event.stopPropagation();
        if (this.disabled) return;
        this.nodeDoubleClicked.emit(node);
    }

    onConnectorMouseDown(event: MouseEvent, node: models.BoardNode): void {
        event.stopPropagation();
        if (this.disabled) return;
        this.connectingFrom = node;
        this.connectMousePos = this.getSvgPoint(event);
    }

    onEdgeClick(event: MouseEvent, edge: models.BoardEdge): void {
        event.stopPropagation();
        if (this.disabled) return;
        this.edgeSelected.emit(edge.id);
    }

    onEdgeDoubleClick(event: MouseEvent, edge: models.BoardEdge): void {
        event.stopPropagation();
        if (this.disabled) return;
        this.edgeDoubleClicked.emit(edge.id);
    }

    onNodeContextMenu(event: MouseEvent, node: models.BoardNode): void {
        event.preventDefault();
        event.stopPropagation();
        if (this.disabled) return;
        this.nodeSelected.emit(node.id);
        this.nodeContextMenu.emit({ event, node });
    }

    onEdgeContextMenu(event: MouseEvent, edge: models.BoardEdge): void {
        event.preventDefault();
        event.stopPropagation();
        if (this.disabled) return;
        this.edgeSelected.emit(edge.id);
        this.edgeContextMenu.emit({ event, edge });
    }

    onEdgeEndpointMouseDown(
        event: MouseEvent,
        edge: models.BoardEdge,
        endpoint: "source" | "target",
    ): void {
        event.stopPropagation();
        if (this.disabled) return;

        this.reconnectingEdge = edge;
        this.reconnectEndpoint = endpoint;
        this.reconnectOriginalNodeId =
            endpoint === "source" ? edge.source_id : edge.target_id;
        this.connectMousePos = this.getSvgPoint(event);

        // Set connectingFrom to the opposite node (the anchor point)
        const anchorNodeId =
            endpoint === "source" ? edge.target_id : edge.source_id;
        this.connectingFrom =
            this.nodes.find((n) => n.id === anchorNodeId) || null;
    }

    onCanvasMouseDown(event: MouseEvent): void {
        const target = event.target as Element;
        const isSvgBackground =
            target === this.svgCanvas?.nativeElement ||
            target.classList.contains("canvas-bg");
        if (isSvgBackground) {
            this.selectionCleared.emit();
            this.isPanning = true;
            this.panStart = { x: event.clientX, y: event.clientY };
        }
    }

    onCanvasMouseMove(event: MouseEvent): void {
        if (this.draggingNode) {
            const svgPoint = this.getSvgPoint(event);
            const newX = svgPoint.x - this.dragOffset.x;
            const newY = svgPoint.y - this.dragOffset.y;
            this.draggingNode.x = newX;
            this.draggingNode.y = newY;
            this.cdr.detectChanges();
        } else if (this.connectingFrom || this.reconnectingEdge) {
            const svgPoint = this.getSvgPoint(event);
            this.connectMousePos = svgPoint;

            // Determine which node to exclude from valid targets
            const excludeNodeId = this.reconnectingEdge
                ? this.reconnectEndpoint === "source"
                    ? this.reconnectingEdge.target_id // Can't connect source to target
                    : this.reconnectingEdge.source_id // Can't connect target to source
                : this.connectingFrom?.id;

            // Detect hover target for magnet highlight
            this.hoverTargetNode =
                this.nodes.find(
                    (n) =>
                        n.id !== excludeNodeId &&
                        svgPoint.x >= n.x &&
                        svgPoint.x <= n.x + NODE_WIDTH &&
                        svgPoint.y >= n.y &&
                        svgPoint.y <= n.y + NODE_HEIGHT,
                ) || null;
            this.cdr.detectChanges();
        } else if (this.isPanning) {
            const dx =
                ((event.clientX - this.panStart.x) * this.viewWidth) /
                (this.svgCanvas?.nativeElement.clientWidth || 900);
            const dy =
                ((event.clientY - this.panStart.y) * this.viewHeight) /
                (this.svgCanvas?.nativeElement.clientHeight || 600);
            this.panX -= dx;
            this.panY -= dy;
            this.panStart = { x: event.clientX, y: event.clientY };
            this.updateViewBox();
        }
    }

    onCanvasMouseUp(event: MouseEvent): void {
        if (this.draggingNode) {
            this.nodeMoved.emit({
                nodeId: this.draggingNode.id,
                x: this.draggingNode.x,
                y: this.draggingNode.y,
            });
            this.draggingNode = null;
        }

        if (this.reconnectingEdge && this.reconnectEndpoint) {
            // Handle edge reconnection
            const svgPoint = this.getSvgPoint(event);
            const excludeNodeId =
                this.reconnectEndpoint === "source"
                    ? this.reconnectingEdge.target_id
                    : this.reconnectingEdge.source_id;

            const targetNode = this.nodes.find(
                (n) =>
                    n.id !== excludeNodeId &&
                    svgPoint.x >= n.x &&
                    svgPoint.x <= n.x + NODE_WIDTH &&
                    svgPoint.y >= n.y &&
                    svgPoint.y <= n.y + NODE_HEIGHT,
            );

            if (targetNode && targetNode.id !== this.reconnectOriginalNodeId) {
                // Emit reconnection event
                this.edgeReconnected.emit({
                    edgeId: this.reconnectingEdge.id,
                    endpoint: this.reconnectEndpoint,
                    newNodeId: targetNode.id,
                });
            }
            // If no valid target, edge stays connected to original node (no action needed)

            this.reconnectingEdge = null;
            this.reconnectEndpoint = null;
            this.reconnectOriginalNodeId = null;
            this.connectingFrom = null;
            this.connectMousePos = null;
            this.hoverTargetNode = null;
            this.cdr.detectChanges();
        } else if (this.connectingFrom) {
            // Find if we're over a node (new connection)
            const svgPoint = this.getSvgPoint(event);
            const targetNode = this.nodes.find(
                (n) =>
                    n.id !== this.connectingFrom!.id &&
                    svgPoint.x >= n.x &&
                    svgPoint.x <= n.x + NODE_WIDTH &&
                    svgPoint.y >= n.y &&
                    svgPoint.y <= n.y + NODE_HEIGHT,
            );
            if (targetNode) {
                this.edgeCreated.emit({
                    sourceId: this.connectingFrom.id,
                    targetId: targetNode.id,
                });
            }
            this.connectingFrom = null;
            this.connectMousePos = null;
            this.hoverTargetNode = null;
            this.cdr.detectChanges();
        }

        this.isPanning = false;
    }

    onCanvasWheel(event: WheelEvent): void {
        event.preventDefault();
        const zoomFactor = event.deltaY > 0 ? 1.1 : 0.9;

        const svgPoint = this.getSvgPoint(event);

        this.panX = svgPoint.x - (svgPoint.x - this.panX) * zoomFactor;
        this.panY = svgPoint.y - (svgPoint.y - this.panY) * zoomFactor;
        this.viewWidth *= zoomFactor;
        this.viewHeight *= zoomFactor;
        this.updateViewBox();
    }

    onDragOver(event: DragEvent): void {
        if (this.disabled) return;
        event.preventDefault();
    }

    onDrop(event: DragEvent): void {
        if (this.disabled) return;
        event.preventDefault();
        const svgPoint = this.getSvgPoint(event);
        this.canvasDrop.emit({ x: svgPoint.x, y: svgPoint.y });
    }

    getEdgeNodes(
        edge: models.BoardEdge,
    ): { source: models.BoardNode; target: models.BoardNode } | null {
        const source = this.nodes.find((n) => n.id === edge.source_id);
        const target = this.nodes.find((n) => n.id === edge.target_id);
        if (!source || !target) return null;
        return { source, target };
    }

    getPath(
        source: models.BoardNode,
        target: models.BoardNode,
        isBidirectional = false,
    ): string {
        return calculateEdgePath(source, target, isBidirectional);
    }

    getMidpoint(
        source: models.BoardNode,
        target: models.BoardNode,
    ): { x: number; y: number } {
        return getEdgeMidpoint(source, target);
    }

    getEdgeColor(edge: models.BoardEdge): string {
        if (this.selectedEdgeId === edge.id) return "#60a5fa";

        if (this.executionStatus) {
            const edgeStatus = this.executionStatus.edge_statuses?.find(
                (es) => es.edge_id === edge.id,
            );
            if (edgeStatus) {
                return EDGE_STATUS_COLORS[edgeStatus.status] || "#6b7280";
            }
        }

        return "#6b7280";
    }

    getMarkerEnd(edge: models.BoardEdge): string {
        if (this.selectedEdgeId === edge.id) return "url(#arrowhead-selected)";

        if (this.executionStatus) {
            const edgeStatus = this.executionStatus.edge_statuses?.find(
                (es) => es.edge_id === edge.id,
            );
            if (edgeStatus && EDGE_STATUS_COLORS[edgeStatus.status]) {
                return `url(#arrowhead-${edgeStatus.status})`;
            }
        }

        return "url(#arrowhead)";
    }

    getMarkerStart(edge: models.BoardEdge): string | null {
        // Only show start marker for bi-sync actions (bi-directional arrow)
        const action = edge.action || "push";
        if (action !== "bi" && action !== "bi-resync") {
            return null;
        }

        if (this.selectedEdgeId === edge.id)
            return "url(#arrowhead-start-selected)";

        if (this.executionStatus) {
            const edgeStatus = this.executionStatus.edge_statuses?.find(
                (es) => es.edge_id === edge.id,
            );
            if (edgeStatus && EDGE_STATUS_COLORS[edgeStatus.status]) {
                return `url(#arrowhead-start-${edgeStatus.status})`;
            }
        }

        return "url(#arrowhead-start)";
    }

    truncatePath(path: string): string {
        if (!path) return "";
        return path.length > 20 ? "..." + path.slice(-17) : path;
    }

    getNodeProviderLabel(node: models.BoardNode): string {
        if (node.remote_name === "local") return "Local";
        return this.remoteProviders.get(node.remote_name || "") || "";
    }

    getActionLabel(action: string | undefined): string {
        switch (action) {
            case "bi":
                return "Bi-Sync";
            case "bi-resync":
                return "Resync";
            case "push":
            default:
                return "Push";
        }
    }

    isBidirectional(edge: models.BoardEdge): boolean {
        const action = edge.action || "push";
        return action === "bi" || action === "bi-resync";
    }

    // Get node status based on connected edges' execution status
    getNodeStatus(node: models.BoardNode): string {
        if (!this.executionStatus?.edge_statuses?.length) return "idle";

        // Find all edges connected to this node
        const connectedEdgeStatuses = this.executionStatus.edge_statuses.filter(
            (es) => {
                const edge = this.edges.find((e) => e.id === es.edge_id);
                return (
                    edge &&
                    (edge.source_id === node.id || edge.target_id === node.id)
                );
            },
        );

        if (!connectedEdgeStatuses.length) return "idle";

        // Priority: running > failed > completed > pending
        if (connectedEdgeStatuses.some((es) => es.status === "running"))
            return "running";
        if (connectedEdgeStatuses.some((es) => es.status === "failed"))
            return "failed";
        if (connectedEdgeStatuses.every((es) => es.status === "completed"))
            return "completed";
        if (connectedEdgeStatuses.some((es) => es.status === "completed"))
            return "completed";
        if (connectedEdgeStatuses.some((es) => es.status === "skipped"))
            return "skipped";

        return "pending";
    }

    getNodeFill(node: models.BoardNode): string {
        if (this.selectedNodeId === node.id) return "#1e3a5f";
        const status = this.getNodeStatus(node);
        if (status === "running") return "#1e3a5f";
        if (status === "completed") return "#14532d";
        if (status === "failed") return "#450a0a";
        return "#1f2937";
    }

    getNodeStroke(node: models.BoardNode): string {
        if (this.selectedNodeId === node.id) return "#3b82f6";
        const status = this.getNodeStatus(node);
        return NODE_STATUS_COLORS[status] || "#374151";
    }

    getNodeStrokeWidth(node: models.BoardNode): number {
        const status = this.getNodeStatus(node);
        if (status === "running") return 3;
        if (this.selectedNodeId === node.id) return 2;
        return 2;
    }

    // Check if node is a valid drop target during connection drag
    isValidConnectionTarget(node: models.BoardNode): boolean {
        // For edge reconnection
        if (this.reconnectingEdge && this.reconnectEndpoint) {
            const excludeNodeId =
                this.reconnectEndpoint === "source"
                    ? this.reconnectingEdge.target_id
                    : this.reconnectingEdge.source_id;
            return node.id !== excludeNodeId;
        }
        // For new connection
        return (
            this.connectingFrom !== null && node.id !== this.connectingFrom.id
        );
    }

    // Check if node is currently being hovered as connection target
    isHoveredTarget(node: models.BoardNode): boolean {
        return this.hoverTargetNode?.id === node.id;
    }

    // Get anchor X position for connection line
    getConnectionAnchorX(): number {
        if (this.reconnectingEdge && this.reconnectEndpoint) {
            // Anchor is the opposite node of the endpoint being dragged
            const anchorNodeId =
                this.reconnectEndpoint === "source"
                    ? this.reconnectingEdge.target_id
                    : this.reconnectingEdge.source_id;
            const anchorNode = this.nodes.find((n) => n.id === anchorNodeId);
            return anchorNode ? anchorNode.x + this.nodeWidth / 2 : 0;
        }
        return this.connectingFrom
            ? this.connectingFrom.x + this.nodeWidth / 2
            : 0;
    }

    // Get anchor Y position for connection line
    getConnectionAnchorY(): number {
        if (this.reconnectingEdge && this.reconnectEndpoint) {
            const anchorNodeId =
                this.reconnectEndpoint === "source"
                    ? this.reconnectingEdge.target_id
                    : this.reconnectingEdge.source_id;
            const anchorNode = this.nodes.find((n) => n.id === anchorNodeId);
            return anchorNode ? anchorNode.y + this.nodeHeight / 2 : 0;
        }
        return this.connectingFrom
            ? this.connectingFrom.y + this.nodeHeight / 2
            : 0;
    }

    private getSvgPoint(event: MouseEvent): { x: number; y: number } {
        const svg = this.svgCanvas?.nativeElement;
        if (!svg) return { x: event.clientX, y: event.clientY };

        const ctm = svg.getScreenCTM();
        if (!ctm) return { x: event.clientX, y: event.clientY };

        const pt = svg.createSVGPoint();
        pt.x = event.clientX;
        pt.y = event.clientY;
        const svgPt = pt.matrixTransform(ctm.inverse());
        return { x: svgPt.x, y: svgPt.y };
    }

    private updateViewBox(): void {
        this.viewBox = `${this.panX} ${this.panY} ${this.viewWidth} ${this.viewHeight}`;
        this.cdr.detectChanges();
    }
}
