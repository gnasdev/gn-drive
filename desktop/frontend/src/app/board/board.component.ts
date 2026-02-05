import { CommonModule } from "@angular/common";
import {
  ChangeDetectionStrategy,
  ChangeDetectorRef,
  Component,
  inject,
  OnDestroy,
  OnInit,
} from "@angular/core";
import { FormsModule } from "@angular/forms";
import { Subscription } from "rxjs";
import { Toolbar } from "primeng/toolbar";
import { ButtonModule } from "primeng/button";
import { Select } from "primeng/select";
import { Dialog } from "primeng/dialog";
import { InputText } from "primeng/inputtext";
import { Card } from "primeng/card";
import { ConfirmationService, MenuItem, MessageService } from "primeng/api";
import { ContextMenu } from "primeng/contextmenu";
import { Tooltip } from "primeng/tooltip";
import * as models from "../../../wailsjs/desktop/backend/models/models.js";
import { AppService } from "../app.service.js";
import { BoardService } from "./board.service.js";
import { BoardCanvasComponent } from "./board-canvas.component.js";
import { EdgeConfigDialogComponent } from "./edge-config-dialog.component.js";
import { PathBrowserComponent } from "../components/path-browser/path-browser.component.js";
import { generateId } from "./board.types.js";

@Component({
  selector: "app-board",
  standalone: true,
  imports: [
    CommonModule,
    FormsModule,
    Toolbar,
    ButtonModule,
    Select,
    Dialog,
    InputText,
    Card,
    ContextMenu,
    Tooltip,
    BoardCanvasComponent,
    EdgeConfigDialogComponent,
    PathBrowserComponent,
  ],
  templateUrl: "./board.component.html",
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class BoardComponent implements OnInit, OnDestroy {
  private subscriptions = new Subscription();

  // View mode: list or editor
  viewMode: "list" | "editor" = "list";

  // Board list
  boards: models.Board[] = [];
  activeBoard: models.Board | null = null;

  // Remote options for adding nodes
  remoteOptions: { label: string; value: string }[] = [];
  selectedRemoteToAdd: string | null = null;

  // Board name dialog
  showNameDialog = false;
  newBoardName = "";

  // Edge config dialog
  showEdgeDialog = false;
  editingEdge: models.BoardEdge | null = null;
  editingEdgeSourceLabel = "";
  editingEdgeTargetLabel = "";

  // Execution
  isExecuting = false;
  executionStatus: models.BoardExecutionStatus | null = null;

  // Context menu
  nodeContextMenuItems: MenuItem[] = [];
  edgeContextMenuItems: MenuItem[] = [];
  private contextNode: models.BoardNode | null = null;
  private contextEdge: models.BoardEdge | null = null;

  // Node edit dialog
  showNodeEditDialog = false;
  editingNode: models.BoardNode | null = null;
  editNodeLabel = "";
  editNodePath = "";

  // Logs
  showLogsDialog = false;
  logsDialogTitle = "";
  logsDialogContent = "";
  logsDialogEdgeId = "";
  latestLogLines = "";

  readonly boardService = inject(BoardService);
  private readonly appService = inject(AppService);
  private readonly cdr = inject(ChangeDetectorRef);
  private readonly confirmationService = inject(ConfirmationService);
  private readonly messageService = inject(MessageService);

  ngOnInit(): void {
    this.boardService.loadBoards();

    this.subscriptions.add(
      this.boardService.boards$.subscribe((boards) => {
        this.boards = boards;
        this.cdr.detectChanges();
      })
    );

    this.subscriptions.add(
      this.boardService.activeBoard$.subscribe((board) => {
        this.activeBoard = board;
        this.cdr.detectChanges();
      })
    );

    this.subscriptions.add(
      this.appService.remotes$.subscribe((remotes) => {
        this.remoteOptions = [
          { label: "Local", value: "local" },
          ...remotes.map((r) => ({ label: r.name || "(Unnamed remote)", value: r.name })),
        ];
        this.cdr.detectChanges();
      })
    );

    this.subscriptions.add(
      this.boardService.executionStatus$.subscribe((status) => {
        this.executionStatus = status;
        this.isExecuting = status?.status === "running";
        this.cdr.detectChanges();
      })
    );

    this.subscriptions.add(
      this.boardService.latestLogLines$.subscribe((lines) => {
        this.latestLogLines = lines;
        // Update logs dialog content if open
        if (this.showLogsDialog && this.logsDialogEdgeId) {
          this.logsDialogContent =
            this.boardService.getEdgeLog(this.logsDialogEdgeId) || "No log output available.";
        }
        this.cdr.detectChanges();
      })
    );
  }

  ngOnDestroy(): void {
    this.subscriptions.unsubscribe();
  }

  // --- List view actions ---

  openBoard(board: models.Board): void {
    this.boardService.setActiveBoard(board.id);
    this.viewMode = "editor";
  }

  backToList(): void {
    this.boardService.activeBoard$.next(null);
    this.boardService.clearSelection();
    this.viewMode = "list";
  }

  createNewBoard(): void {
    this.newBoardName = "";
    this.showNameDialog = true;
  }

  confirmCreateBoard(): void {
    if (!this.newBoardName.trim()) return;

    const board = new models.Board();
    board.id = generateId("board");
    board.name = this.newBoardName.trim();
    board.nodes = [];
    board.edges = [];

    this.boardService.saveBoard(board).then(() => {
      this.boardService.setActiveBoard(board.id);
      this.viewMode = "editor";
    });

    this.showNameDialog = false;
  }

  deleteBoardConfirm(board: models.Board): void {
    this.confirmationService.confirm({
      message: `Delete board "${board.name || "(Unnamed)"}"?`,
      header: "Confirm Delete",
      icon: "pi pi-exclamation-triangle",
      accept: () => {
        this.boardService.deleteBoard(board.id);
      },
    });
  }

  // --- Editor view actions ---

  addNodeFromRemote(remoteName: string): void {
    if (!remoteName) return;
    const label = remoteName === "local" ? "Local" : remoteName;
    this.boardService.addNode(remoteName, "", label);
    this.selectedRemoteToAdd = null;
  }

  async saveBoard(): Promise<void> {
    if (this.activeBoard) {
      await this.boardService.saveBoard(this.activeBoard);
      this.messageService.add({
        severity: "success",
        summary: "Saved",
        detail: `Board "${this.activeBoard.name || "(Unnamed)"}" saved successfully.`,
        life: 3000,
      });
    }
  }

  async executeBoard(): Promise<void> {
    if (this.activeBoard) {
      await this.boardService.saveBoard(this.activeBoard);
      await this.boardService.executeBoard(this.activeBoard.id);
    }
  }

  async stopExecution(): Promise<void> {
    if (this.activeBoard) {
      await this.boardService.stopExecution(this.activeBoard.id);
    }
  }

  openEdgeDialog(edgeId: string | null): void {
    if (!edgeId || !this.activeBoard) return;
    this.boardService.selectEdge(edgeId);
    const edge = this.activeBoard.edges?.find((e) => e.id === edgeId);
    if (edge) {
      if (!edge.sync_config) {
        edge.sync_config = new models.Profile();
      }
      this.editingEdge = edge;
      this.editingEdgeSourceLabel = this.getNodeLabel(edge.source_id);
      this.editingEdgeTargetLabel = this.getNodeLabel(edge.target_id);
      this.showEdgeDialog = true;
    }
  }

  onEdgeSaved(edge: models.BoardEdge): void {
    this.boardService.updateEdgeConfig(edge.id, edge);
    this.showEdgeDialog = false;
  }

  onEdgeDeleted(edgeId: string): void {
    this.boardService.removeEdge(edgeId);
    this.showEdgeDialog = false;
  }

  getNodeLabel(nodeId: string): string {
    const node = this.activeBoard?.nodes?.find((n) => n.id === nodeId);
    return node?.label || node?.remote_name || nodeId;
  }

  getEdgeLabel(edgeId: string): string {
    const edge = this.activeBoard?.edges?.find((e) => e.id === edgeId);
    if (!edge) return edgeId;
    const src = this.getNodeLabel(edge.source_id);
    const tgt = this.getNodeLabel(edge.target_id);
    return `${src} -> ${tgt}`;
  }

  getBoardSummary(board: models.Board): string {
    const nodes = board.nodes?.length || 0;
    const edges = board.edges?.length || 0;
    return `${nodes} node${nodes !== 1 ? "s" : ""}, ${edges} edge${edges !== 1 ? "s" : ""}`;
  }

  // --- Context menu handlers ---

  onNodeContextMenu(event: { event: MouseEvent; node: models.BoardNode }, cm: ContextMenu): void {
    this.contextNode = event.node;
    this.nodeContextMenuItems = [
      {
        label: "Edit Node",
        icon: "pi pi-pencil",
        command: () => this.openNodeEditDialog(this.contextNode!),
      },
      {
        label: "Delete Node",
        icon: "pi pi-trash",
        command: () => {
          if (this.contextNode) {
            this.boardService.removeNode(this.contextNode.id);
          }
        },
      },
    ];
    cm.show(event.event);
  }

  onEdgeContextMenu(event: { event: MouseEvent; edge: models.BoardEdge }, cm: ContextMenu): void {
    this.contextEdge = event.edge;
    this.edgeContextMenuItems = [
      {
        label: "Configure Edge",
        icon: "pi pi-cog",
        command: () => {
          if (this.contextEdge) {
            this.openEdgeDialog(this.contextEdge.id);
          }
        },
      },
      {
        label: "Delete Edge",
        icon: "pi pi-trash",
        command: () => {
          if (this.contextEdge) {
            this.boardService.removeEdge(this.contextEdge.id);
          }
        },
      },
    ];
    cm.show(event.event);
  }

  // --- Node edit dialog ---

  openNodeEditDialog(node: models.BoardNode): void {
    this.editingNode = node;
    this.editNodeLabel = node.label || "";
    this.editNodePath = node.path || "";
    this.showNodeEditDialog = true;
  }

  saveNodeEdit(): void {
    if (!this.editingNode || !this.activeBoard) return;
    const node = this.activeBoard.nodes?.find((n) => n.id === this.editingNode!.id);
    if (node) {
      node.label = this.editNodeLabel.trim() || node.remote_name;
      node.path = this.editNodePath.trim();
      this.boardService.updateNodeInPlace(this.activeBoard);
    }
    this.showNodeEditDialog = false;
  }

  // --- Logs dialog ---

  showEdgeLogs(es: models.EdgeExecutionStatus): void {
    const edge = this.activeBoard?.edges?.find((e) => e.id === es.edge_id);
    this.logsDialogTitle = edge ? this.getEdgeLabel(edge.id) : es.edge_id;
    this.logsDialogEdgeId = es.edge_id;
    const edgeLog = this.boardService.getEdgeLog(es.edge_id);
    this.logsDialogContent = edgeLog || es.message || "No log output available.";
    this.showLogsDialog = true;
  }

  getStatusBadgeClass(status: string): string {
    switch (status) {
      case "running":
        return "bg-blue-500/20 text-blue-400";
      case "completed":
        return "bg-green-500/20 text-green-400";
      case "failed":
        return "bg-red-500/20 text-red-400";
      case "cancelled":
        return "bg-yellow-500/20 text-yellow-400";
      case "skipped":
        return "bg-orange-500/20 text-orange-400";
      case "pending":
        return "bg-gray-500/20 text-gray-400";
      default:
        return "bg-gray-500/20 text-gray-400";
    }
  }
}
