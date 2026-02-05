import { CommonModule } from "@angular/common";
import {
  ChangeDetectionStrategy,
  ChangeDetectorRef,
  Component,
  ElementRef,
  EventEmitter,
  inject,
  Input,
  Output,
  ViewChild,
} from "@angular/core";
import * as models from "../../../wailsjs/desktop/backend/models/models.js";
import {
  NODE_WIDTH,
  NODE_HEIGHT,
  NODE_HANDLE_RADIUS,
  EDGE_STATUS_COLORS,
  calculateEdgePath,
  getEdgeMidpoint,
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
    >
      <defs>
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
          <polygon points="0 0, 10 3.5, 0 7" [attr.fill]="entry[1]" />
        </marker>
        }
      </defs>

      <!-- Grid pattern -->
      <defs>
        <pattern id="grid" width="20" height="20" patternUnits="userSpaceOnUse">
          <path
            d="M 20 0 L 0 0 0 20"
            fill="none"
            stroke="#1f2937"
            stroke-width="0.5"
          />
        </pattern>
      </defs>
      <rect class="canvas-bg" width="5000" height="5000" x="-2500" y="-2500" fill="url(#grid)" />

      <!-- Edges -->
      @for (edge of edges; track edge.id) {
      @if (getEdgeNodes(edge); as nodes) {
      <g class="cursor-pointer" (click)="onEdgeClick($event, edge)" (dblclick)="onEdgeDoubleClick($event, edge)" (contextmenu)="onEdgeContextMenu($event, edge)">
        <path
          [attr.d]="getPath(nodes.source, nodes.target)"
          fill="none"
          [attr.stroke]="getEdgeColor(edge)"
          [attr.stroke-width]="selectedEdgeId === edge.id ? 3 : 2"
          [attr.marker-end]="getMarkerEnd(edge)"
          stroke-linecap="round"
        />
        <!-- Invisible wider path for easier clicking -->
        <path
          [attr.d]="getPath(nodes.source, nodes.target)"
          fill="none"
          stroke="transparent"
          stroke-width="12"
        />
        <!-- Edge label -->
        <text
          [attr.x]="getMidpoint(nodes.source, nodes.target).x"
          [attr.y]="getMidpoint(nodes.source, nodes.target).y - 8"
          text-anchor="middle"
          class="text-xs select-none pointer-events-none"
          fill="#9ca3af"
          font-size="11"
        >
          {{ edge.action || 'push' }}
        </text>
      </g>
      } }

      <!-- Connection line while dragging -->
      @if (connectingFrom && connectMousePos) {
      <line
        [attr.x1]="connectingFrom.x + nodeWidth / 2"
        [attr.y1]="connectingFrom.y + nodeHeight / 2"
        [attr.x2]="connectMousePos.x"
        [attr.y2]="connectMousePos.y"
        stroke="#60a5fa"
        stroke-width="2"
        stroke-dasharray="5,5"
      />
      }

      <!-- Nodes -->
      @for (node of nodes; track node.id) {
      <g
        [attr.transform]="'translate(' + node.x + ',' + node.y + ')'"
        class="cursor-move"
        (mousedown)="onNodeMouseDown($event, node)"
        (dblclick)="onNodeDoubleClick($event, node)"
        (contextmenu)="onNodeContextMenu($event, node)"
      >
        <!-- Node background -->
        <rect
          [attr.width]="nodeWidth"
          [attr.height]="nodeHeight"
          rx="8"
          ry="8"
          [attr.fill]="selectedNodeId === node.id ? '#1e3a5f' : '#1f2937'"
          [attr.stroke]="selectedNodeId === node.id ? '#3b82f6' : '#374151'"
          stroke-width="2"
        />
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
          {{ node.label || node.remote_name || '(No name)' }}
        </text>
        <!-- Path subtitle -->
        <text
          [attr.x]="nodeWidth / 2"
          y="44"
          text-anchor="middle"
          fill="#6b7280"
          font-size="10"
          class="select-none pointer-events-none"
        >
          {{ truncatePath(node.path) || '(No path)' }}
        </text>
        <!-- Connection handle (right side) -->
        <circle
          [attr.cx]="nodeWidth"
          [attr.cy]="nodeHeight / 2"
          [attr.r]="handleRadius"
          fill="#3b82f6"
          stroke="#1e3a5f"
          stroke-width="2"
          class="cursor-crosshair"
          (mousedown)="onHandleMouseDown($event, node)"
        />
        <!-- Connection handle (left side) -->
        <circle
          cx="0"
          [attr.cy]="nodeHeight / 2"
          [attr.r]="handleRadius"
          fill="#3b82f6"
          stroke="#1e3a5f"
          stroke-width="2"
          class="cursor-crosshair"
        />
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

  readonly nodeWidth = NODE_WIDTH;
  readonly nodeHeight = NODE_HEIGHT;
  readonly handleRadius = NODE_HANDLE_RADIUS;
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

  private readonly cdr = inject(ChangeDetectorRef);

  onNodeMouseDown(event: MouseEvent, node: models.BoardNode): void {
    event.stopPropagation();
    this.nodeSelected.emit(node.id);

    const svgPoint = this.getSvgPoint(event);
    this.draggingNode = node;
    this.dragOffset = {
      x: svgPoint.x - node.x,
      y: svgPoint.y - node.y,
    };
  }

  onNodeDoubleClick(event: MouseEvent, node: models.BoardNode): void {
    event.stopPropagation();
    this.nodeDoubleClicked.emit(node);
  }

  onHandleMouseDown(event: MouseEvent, node: models.BoardNode): void {
    event.stopPropagation();
    this.connectingFrom = node;
    this.connectMousePos = this.getSvgPoint(event);
  }

  onEdgeClick(event: MouseEvent, edge: models.BoardEdge): void {
    event.stopPropagation();
    this.edgeSelected.emit(edge.id);
  }

  onEdgeDoubleClick(event: MouseEvent, edge: models.BoardEdge): void {
    event.stopPropagation();
    this.edgeDoubleClicked.emit(edge.id);
  }

  onNodeContextMenu(event: MouseEvent, node: models.BoardNode): void {
    event.preventDefault();
    event.stopPropagation();
    this.nodeSelected.emit(node.id);
    this.nodeContextMenu.emit({ event, node });
  }

  onEdgeContextMenu(event: MouseEvent, edge: models.BoardEdge): void {
    event.preventDefault();
    event.stopPropagation();
    this.edgeSelected.emit(edge.id);
    this.edgeContextMenu.emit({ event, edge });
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
    } else if (this.connectingFrom) {
      this.connectMousePos = this.getSvgPoint(event);
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

    if (this.connectingFrom) {
      // Find if we're over a node
      const svgPoint = this.getSvgPoint(event);
      const targetNode = this.nodes.find(
        (n) =>
          n.id !== this.connectingFrom!.id &&
          svgPoint.x >= n.x &&
          svgPoint.x <= n.x + NODE_WIDTH &&
          svgPoint.y >= n.y &&
          svgPoint.y <= n.y + NODE_HEIGHT
      );
      if (targetNode) {
        this.edgeCreated.emit({
          sourceId: this.connectingFrom.id,
          targetId: targetNode.id,
        });
      }
      this.connectingFrom = null;
      this.connectMousePos = null;
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

  getEdgeNodes(
    edge: models.BoardEdge
  ): { source: models.BoardNode; target: models.BoardNode } | null {
    const source = this.nodes.find((n) => n.id === edge.source_id);
    const target = this.nodes.find((n) => n.id === edge.target_id);
    if (!source || !target) return null;
    return { source, target };
  }

  getPath(source: models.BoardNode, target: models.BoardNode): string {
    return calculateEdgePath(source, target);
  }

  getMidpoint(
    source: models.BoardNode,
    target: models.BoardNode
  ): { x: number; y: number } {
    return getEdgeMidpoint(source, target);
  }

  getEdgeColor(edge: models.BoardEdge): string {
    if (this.selectedEdgeId === edge.id) return "#60a5fa";

    if (this.executionStatus) {
      const edgeStatus = this.executionStatus.edge_statuses?.find(
        (es) => es.edge_id === edge.id
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
        (es) => es.edge_id === edge.id
      );
      if (edgeStatus && EDGE_STATUS_COLORS[edgeStatus.status]) {
        return `url(#arrowhead-${edgeStatus.status})`;
      }
    }

    return "url(#arrowhead)";
  }

  truncatePath(path: string): string {
    if (!path) return "";
    return path.length > 20 ? "..." + path.slice(-17) : path;
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
