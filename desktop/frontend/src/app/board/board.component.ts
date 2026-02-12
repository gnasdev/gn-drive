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
import { ConfirmationService, MenuItem } from "primeng/api";
import { ButtonModule } from "primeng/button";
import { Card } from "primeng/card";
import { ContextMenu } from "primeng/contextmenu";
import { Dialog } from "primeng/dialog";
import { InputText } from "primeng/inputtext";
import { Menu } from "primeng/menu";
import { Select } from "primeng/select";
import { ToggleSwitch } from "primeng/toggleswitch";
import { Toolbar } from "primeng/toolbar";
import { Tooltip } from "primeng/tooltip";
import { Subscription } from "rxjs";
import * as models from "../../../wailsjs/desktop/backend/models/models.js";
import { AppService } from "../app.service.js";
import { PathBrowserComponent } from "../components/path-browser/path-browser.component.js";
import { REMOTE_TYPE_OPTIONS } from "../remotes/remotes.types.js";
import { BoardCanvasComponent } from "./board-canvas.component.js";
import { BoardService } from "./board.service.js";
import { generateId } from "./board.types.js";
import { EdgeConfigDialogComponent } from "./edge-config-dialog.component.js";

@Component({
    selector: "app-board",
    standalone: true,
    imports: [
        CommonModule,
        FormsModule,
        Toolbar,
        ButtonModule,
        Dialog,
        InputText,
        Card,
        ContextMenu,
        Tooltip,
        Menu,
        Select,
        ToggleSwitch,
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

    // Remote options for adding nodes (as draggable badges)
    remoteOptions: { label: string; value: string }[] = [];
    availableRemotes: { label: string; value: string }[] = []; // Remotes not yet added to board
    remoteProviders = new Map<string, string>(); // remote name -> provider label

    // Board name dialog (for creating new board)
    showNameDialog = false;
    newBoardName = "";

    // Edit board name dialog
    showEditNameDialog = false;
    editBoardName = "";

    // Edge config dialog
    showEdgeDialog = false;
    editingEdge: models.BoardEdge | null = null;
    editingEdgeSourceLabel = "";
    editingEdgeTargetLabel = "";
    editingEdgeSourceProvider = "";
    editingEdgeTargetProvider = "";
    editingEdgeSourceType = "";
    editingEdgeTargetType = "";

    // Execution
    isExecuting = false;
    executionStatus: models.BoardExecutionStatus | null = null;

    // Context menu
    nodeContextMenuItems: MenuItem[] = [];
    edgeContextMenuItems: MenuItem[] = [];
    boardSettingsMenuItems: MenuItem[] = [];
    private contextNode: models.BoardNode | null = null;
    private contextEdge: models.BoardEdge | null = null;

    // Node edit dialog
    showNodeEditDialog = false;
    editingNode: models.BoardNode | null = null;
    editNodePath = "";

    // Logs
    showLogsDialog = false;
    logsDialogTitle = "";
    logsDialogContent = "";
    logsDialogEdgeId = "";
    latestLogLines = "";

    // Schedule dialog
    showScheduleDialog = false;
    scheduleEnabled = false;
    scheduleCronExpr = "0 */6 * * *";
    useCustomCron = false;
    cronParts = { minute: "0", hour: "*/6", dom: "*", month: "*", dow: "*" };

    readonly frequencyPresets = [
        { label: "Every hour", value: "0 * * * *" },
        { label: "Every 2 hours", value: "0 */2 * * *" },
        { label: "Every 6 hours", value: "0 */6 * * *" },
        { label: "Every 12 hours", value: "0 */12 * * *" },
        { label: "Daily at midnight", value: "0 0 * * *" },
        { label: "Daily at 6 AM", value: "0 6 * * *" },
        { label: "Daily at noon", value: "0 12 * * *" },
        { label: "Weekly (Monday midnight)", value: "0 0 * * 1" },
        { label: "Monthly (1st at midnight)", value: "0 0 1 * *" },
    ];

    readonly minuteOptions = [
        { label: "Every minute (*)", value: "*" },
        { label: "0", value: "0" },
        { label: "5", value: "5" },
        { label: "10", value: "10" },
        { label: "15", value: "15" },
        { label: "30", value: "30" },
        { label: "Every 5 min (*/5)", value: "*/5" },
        { label: "Every 15 min (*/15)", value: "*/15" },
        { label: "Every 30 min (*/30)", value: "*/30" },
    ];

    readonly hourOptions = [
        { label: "Every hour (*)", value: "*" },
        { label: "0 (midnight)", value: "0" },
        { label: "6 (6 AM)", value: "6" },
        { label: "12 (noon)", value: "12" },
        { label: "18 (6 PM)", value: "18" },
        { label: "Every 2h (*/2)", value: "*/2" },
        { label: "Every 6h (*/6)", value: "*/6" },
        { label: "Every 12h (*/12)", value: "*/12" },
    ];

    readonly domOptions = [
        { label: "Every day (*)", value: "*" },
        { label: "1st", value: "1" },
        { label: "15th", value: "15" },
    ];

    readonly monthOptions = [{ label: "Every month (*)", value: "*" }];

    readonly dowOptions = [
        { label: "Every day (*)", value: "*" },
        { label: "Monday", value: "1" },
        { label: "Weekdays (1-5)", value: "1-5" },
        { label: "Weekend (0,6)", value: "0,6" },
    ];

    readonly boardService = inject(BoardService);
    private readonly appService = inject(AppService);
    private readonly cdr = inject(ChangeDetectorRef);
    private readonly confirmationService = inject(ConfirmationService);

    ngOnInit(): void {
        this.boardService.loadBoards();
        this.initBoardSettingsMenu();

        this.subscriptions.add(
            this.boardService.boards$.subscribe((boards) => {
                this.boards = boards;
                this.cdr.markForCheck();
            }),
        );

        this.subscriptions.add(
            this.boardService.activeBoard$.subscribe((board) => {
                this.activeBoard = board;
                this.updateAvailableRemotes();
                this.cdr.markForCheck();
            }),
        );

        this.subscriptions.add(
            this.appService.remotes$.subscribe((remotes) => {
                this.remoteOptions = [
                    { label: "Local", value: "local" },
                    ...remotes.map((r) => ({
                        label: r.name || "(Unnamed remote)",
                        value: r.name,
                    })),
                ];
                // Build remote providers map
                this.remoteProviders = new Map<string, string>();
                this.remoteProviders.set("local", "Local");
                for (const remote of remotes) {
                    if (remote.name) {
                        this.remoteProviders.set(
                            remote.name,
                            this.getProviderLabel(remote.type),
                        );
                    }
                }
                this.updateAvailableRemotes();
                this.cdr.markForCheck();
            }),
        );

        this.subscriptions.add(
            this.boardService.executionStatus$.subscribe((status) => {
                this.executionStatus = status;
                this.isExecuting = status?.status === "running";
                this.cdr.markForCheck();
            }),
        );

        this.subscriptions.add(
            this.boardService.latestLogLines$.subscribe((lines) => {
                this.latestLogLines = lines;
                // Update logs dialog content if open
                if (this.showLogsDialog && this.logsDialogEdgeId) {
                    this.logsDialogContent =
                        this.boardService.getEdgeLog(this.logsDialogEdgeId) ||
                        "No log output available.";
                }
                this.cdr.markForCheck();
            }),
        );
    }

    ngOnDestroy(): void {
        this.subscriptions.unsubscribe();
    }

    private initBoardSettingsMenu(): void {
        this.boardSettingsMenuItems = [
            {
                label: "Schedule",
                icon: "pi pi-calendar",
                command: () => this.openScheduleDialog(),
            },
            {
                label: "Rename",
                icon: "pi pi-pencil",
                command: () => this.openEditNameDialog(),
            },
            { separator: true },
            {
                label: "Delete Board",
                icon: "pi pi-trash",
                styleClass: "text-red-400",
                command: () => this.deleteBoardFromEditor(),
            },
        ];
    }

    // --- List view actions ---

    async openBoard(board: models.Board): Promise<void> {
        await this.boardService.setActiveBoard(board.id);
        this.viewMode = "editor";
        this.cdr.markForCheck();
    }

    backToList(): void {
        this.boardService.activeBoard$.next(null);
        this.boardService.clearSelection();
        this.viewMode = "list";
        this.cdr.markForCheck();
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

    openEditNameDialog(): void {
        if (!this.activeBoard) return;
        this.editBoardName = this.activeBoard.name || "";
        this.showEditNameDialog = true;
    }

    confirmEditBoardName(): void {
        if (!this.activeBoard || !this.editBoardName.trim()) return;
        this.activeBoard.name = this.editBoardName.trim();
        this.boardService.saveBoard(this.activeBoard);
        this.showEditNameDialog = false;
    }

    deleteBoardFromEditor(): void {
        if (!this.activeBoard) return;
        this.confirmationService.confirm({
            message: `Delete board "${this.activeBoard.name || "(Unnamed)"}"?`,
            header: "Confirm Delete",
            icon: "pi pi-exclamation-triangle",
            accept: () => {
                if (this.activeBoard) {
                    this.boardService.deleteBoard(this.activeBoard.id);
                    this.backToList();
                }
            },
        });
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

    // Update available remotes (remotes not yet added to board)
    private updateAvailableRemotes(): void {
        if (!this.activeBoard) {
            this.availableRemotes = [...this.remoteOptions];
            return;
        }
        const usedRemotes = new Set(
            this.activeBoard.nodes?.map((n) => n.remote_name) || [],
        );
        this.availableRemotes = this.remoteOptions.filter(
            (r) => !usedRemotes.has(r.value),
        );
    }

    onRemoteClick(remote: { label: string; value: string }): void {
        if (this.isExecuting) return;
        const label = remote.value === "local" ? "Local" : remote.value;
        this.boardService.addNode(remote.value, "", label, 350, 200);
        this.updateAvailableRemotes();
        this.cdr.markForCheck();
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
            this.editingEdgeSourceProvider = this.getNodeProvider(
                edge.source_id,
            );
            this.editingEdgeTargetProvider = this.getNodeProvider(
                edge.target_id,
            );
            this.editingEdgeSourceType = this.getNodeType(edge.source_id);
            this.editingEdgeTargetType = this.getNodeType(edge.target_id);
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

    getNodeProvider(nodeId: string): string {
        const node = this.activeBoard?.nodes?.find((n) => n.id === nodeId);
        if (!node) return "";
        if (node.remote_name === "local") return "Local Filesystem";
        const remote = this.appService.remotes$.value?.find(
            (r) => r.name === node.remote_name,
        );
        if (!remote) return "";
        return this.getProviderLabel(remote.type);
    }

    getNodeType(nodeId: string): string {
        const node = this.activeBoard?.nodes?.find((n) => n.id === nodeId);
        if (!node) return "";
        if (node.remote_name === "local") return "local";
        const remote = this.appService.remotes$.value?.find(
            (r) => r.name === node.remote_name,
        );
        return remote?.type || "";
    }

    getProviderLabel(type: string): string {
        const option = REMOTE_TYPE_OPTIONS.find((opt) => opt.value === type);
        return option?.label || type || "";
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

    onNodeContextMenu(
        event: { event: MouseEvent; node: models.BoardNode },
        cm: ContextMenu,
    ): void {
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

    onEdgeContextMenu(
        event: { event: MouseEvent; edge: models.BoardEdge },
        cm: ContextMenu,
    ): void {
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
        this.editNodePath = node.path || "";
        this.showNodeEditDialog = true;
    }

    saveNodeEdit(): void {
        if (!this.editingNode || !this.activeBoard) return;
        const node = this.activeBoard.nodes?.find(
            (n) => n.id === this.editingNode!.id,
        );
        if (node) {
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
        this.logsDialogContent =
            edgeLog || es.message || "No log output available.";
        this.showLogsDialog = true;
    }

    getStatusBadgeClass(status: string): string {
        switch (status) {
            case "running":
                return "bg-blue-500/20 text-blue-400";
            case "completed":
            case "success":
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

    // --- Schedule methods ---

    openScheduleDialog(): void {
        if (!this.activeBoard) return;
        this.scheduleEnabled = this.activeBoard.schedule_enabled || false;
        this.scheduleCronExpr = this.activeBoard.cron_expr || "0 */6 * * *";
        this.parseCronToParts(this.scheduleCronExpr);
        this.useCustomCron = false;
        this.showScheduleDialog = true;
    }

    saveSchedule(): void {
        if (!this.activeBoard) return;
        this.activeBoard.schedule_enabled = this.scheduleEnabled;
        this.activeBoard.cron_expr = this.scheduleCronExpr;
        this.boardService.saveBoard(this.activeBoard);
        this.showScheduleDialog = false;
    }

    onPresetChange(cronExpr: string): void {
        this.scheduleCronExpr = cronExpr;
        this.parseCronToParts(cronExpr);
    }

    onCronPartChange(): void {
        const { minute, hour, dom, month, dow } = this.cronParts;
        this.scheduleCronExpr = `${minute} ${hour} ${dom} ${month} ${dow}`;
    }

    parseCronToParts(expr: string): void {
        const parts = expr.split(" ");
        if (parts.length === 5) {
            this.cronParts = {
                minute: parts[0],
                hour: parts[1],
                dom: parts[2],
                month: parts[3],
                dow: parts[4],
            };
        }
    }

    getCronDescription(): string {
        const { minute, hour, dom, month, dow } = this.cronParts;
        const parts: string[] = [];

        if (minute === "*") parts.push("every minute");
        else if (minute.startsWith("*/"))
            parts.push(`every ${minute.slice(2)} minutes`);
        else parts.push(`at minute ${minute}`);

        if (hour !== "*") {
            if (hour.startsWith("*/"))
                parts.push(`every ${hour.slice(2)} hours`);
            else parts.push(`at ${hour}:00`);
        }

        if (dom !== "*") parts.push(`on day ${dom}`);

        if (month !== "*") {
            const monthNames = [
                "",
                "Jan",
                "Feb",
                "Mar",
                "Apr",
                "May",
                "Jun",
                "Jul",
                "Aug",
                "Sep",
                "Oct",
                "Nov",
                "Dec",
            ];
            const m = parseInt(month, 10);
            parts.push(`in ${m >= 1 && m <= 12 ? monthNames[m] : month}`);
        }

        if (dow !== "*") {
            const dayMap: Record<string, string> = {
                "0": "Sunday",
                "1": "Monday",
                "2": "Tuesday",
                "3": "Wednesday",
                "4": "Thursday",
                "5": "Friday",
                "6": "Saturday",
                "1-5": "weekdays",
                "0,6": "weekends",
            };
            parts.push(`on ${dayMap[dow] || dow}`);
        }

        return parts.join(", ");
    }
}
