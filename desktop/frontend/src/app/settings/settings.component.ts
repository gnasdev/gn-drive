import { CommonModule } from "@angular/common";
import {
    ChangeDetectionStrategy,
    ChangeDetectorRef,
    Component,
    OnInit,
    inject,
} from "@angular/core";
import { FormsModule } from "@angular/forms";
import { MessageService } from "primeng/api";
import { Button } from "primeng/button";
import { Card } from "primeng/card";
import { Checkbox } from "primeng/checkbox";
import { Dialog } from "primeng/dialog";
import { ToggleSwitch } from "primeng/toggleswitch";
import { Toolbar } from "primeng/toolbar";
import {
    ExportToFile,
    GetExportPreview,
    SelectExportFile,
} from "../../../wailsjs/desktop/backend/services/exportservice";
import {
    ImportFromFile,
    SelectImportFile,
    ValidateImportFile,
} from "../../../wailsjs/desktop/backend/services/importservice";
import type {
    ExportOptions,
    ImportOptions,
    ImportPreview,
} from "../../../wailsjs/desktop/backend/services/models";
import {
    GetSettings,
    SetEnabled,
    SetMinimizeToTray,
    SetStartAtLogin,
} from "../../../wailsjs/desktop/backend/services/notificationservice";

@Component({
    selector: "app-settings",
    standalone: true,
    imports: [
        CommonModule,
        FormsModule,
        Card,
        Toolbar,
        ToggleSwitch,
        Button,
        Dialog,
        Checkbox,
    ],
    changeDetection: ChangeDetectionStrategy.OnPush,
    template: `
        <div class="flex flex-col h-full">
            <!-- Header -->
            <p-toolbar>
                <ng-template #start>
                    <div class="flex items-center gap-3">
                        <i class="pi pi-cog text-primary-400"></i>
                        <h1 class="text-lg font-semibold text-gray-100">
                            Settings
                        </h1>
                    </div>
                </ng-template>
            </p-toolbar>

            <!-- Settings sections -->
            <div class="flex-1 overflow-auto p-4 space-y-4">
                <!-- Backup & Restore -->
                <p-card>
                    <div class="flex items-center gap-2 mb-3">
                        <i class="pi pi-download text-gray-400"></i>
                        <h2 class="text-sm font-semibold text-gray-200">
                            Backup & Restore
                        </h2>
                    </div>
                    <div class="space-y-4">
                        <div
                            class="flex items-center justify-between gap-4 flex-wrap"
                        >
                            <div class="flex-1 min-w-[200px]">
                                <p class="text-sm text-gray-300">
                                    Export Configuration
                                </p>
                                <p class="text-xs text-gray-500">
                                    Save boards, remotes, and settings to a
                                    backup file
                                </p>
                            </div>
                            <p-button
                                label="Export"
                                icon="pi pi-upload"
                                severity="secondary"
                                [outlined]="true"
                                size="small"
                                (onClick)="openExportDialog()"
                            ></p-button>
                        </div>
                        <div
                            class="flex items-center justify-between gap-4 flex-wrap"
                        >
                            <div class="flex-1 min-w-[200px]">
                                <p class="text-sm text-gray-300">
                                    Import Configuration
                                </p>
                                <p class="text-xs text-gray-500">
                                    Restore boards and remotes from a backup
                                    file
                                </p>
                            </div>
                            <p-button
                                label="Import"
                                icon="pi pi-download"
                                severity="secondary"
                                [outlined]="true"
                                size="small"
                                (onClick)="openImportDialog()"
                            ></p-button>
                        </div>
                    </div>
                </p-card>

                <!-- Startup -->
                <p-card>
                    <div class="flex items-center gap-2 mb-3">
                        <i class="pi pi-power-off text-gray-400"></i>
                        <h2 class="text-sm font-semibold text-gray-200">
                            Startup
                        </h2>
                    </div>
                    <div class="space-y-4">
                        <div class="flex items-center justify-between">
                            <div>
                                <p class="text-sm text-gray-300">
                                    Start at Login
                                </p>
                                <p class="text-xs text-gray-500">
                                    Automatically start GN Drive when you log in
                                </p>
                            </div>
                            <p-toggleswitch
                                [(ngModel)]="startAtLogin"
                                (onChange)="saveStartAtLoginSetting()"
                            ></p-toggleswitch>
                        </div>
                        <div class="flex items-center justify-between">
                            <div>
                                <p class="text-sm text-gray-300">
                                    Minimize to Tray
                                </p>
                                <p class="text-xs text-gray-500">
                                    Keep app running in system tray when window
                                    is closed
                                </p>
                            </div>
                            <p-toggleswitch
                                [(ngModel)]="minimizeToTray"
                                (onChange)="saveMinimizeToTraySetting()"
                            ></p-toggleswitch>
                        </div>
                    </div>
                </p-card>

                <!-- Notifications -->
                <p-card>
                    <div class="flex items-center gap-2 mb-3">
                        <i class="pi pi-bell text-gray-400"></i>
                        <h2 class="text-sm font-semibold text-gray-200">
                            Notifications
                        </h2>
                    </div>
                    <div class="flex items-center justify-between">
                        <div>
                            <p class="text-sm text-gray-300">
                                Desktop Notifications
                            </p>
                            <p class="text-xs text-gray-500">
                                Show notifications when operations complete or
                                fail
                            </p>
                        </div>
                        <p-toggleswitch
                            [(ngModel)]="notificationsEnabled"
                            (onChange)="saveNotificationSetting()"
                        ></p-toggleswitch>
                    </div>
                </p-card>

                <!-- Security -->
                <p-card>
                    <div class="flex items-center gap-2 mb-3">
                        <i class="pi pi-lock text-gray-400"></i>
                        <h2 class="text-sm font-semibold text-gray-200">
                            Security
                        </h2>
                    </div>
                    <div class="flex items-center justify-between">
                        <div>
                            <p class="text-sm text-gray-300">
                                Config Encryption
                            </p>
                            <p class="text-xs text-gray-500">
                                Encrypt rclone configuration file with a
                                password
                            </p>
                        </div>
                        <p-toggleswitch
                            [(ngModel)]="configEncrypted"
                            (onChange)="toggleConfigEncryption()"
                        ></p-toggleswitch>
                    </div>
                </p-card>

                <!-- Paths -->
                <p-card>
                    <div class="flex items-center gap-2 mb-3">
                        <i class="pi pi-folder-open text-gray-400"></i>
                        <h2 class="text-sm font-semibold text-gray-200">
                            Configuration Paths
                        </h2>
                    </div>
                    <div class="space-y-2">
                        <div class="flex justify-between text-sm">
                            <span class="text-gray-400">Boards</span>
                            <span class="text-gray-500 font-mono text-xs"
                                >~/.config/gn-drive/boards.json</span
                            >
                        </div>
                        <div class="flex justify-between text-sm">
                            <span class="text-gray-400">Rclone Config</span>
                            <span class="text-gray-500 font-mono text-xs"
                                >~/.config/gn-drive/rclone.conf</span
                            >
                        </div>
                    </div>
                </p-card>

                <!-- About -->
                <p-card>
                    <div class="flex items-center gap-2 mb-3">
                        <i class="pi pi-info-circle text-gray-400"></i>
                        <h2 class="text-sm font-semibold text-gray-200">
                            About
                        </h2>
                    </div>
                    <div class="space-y-2">
                        <div class="flex justify-between text-sm">
                            <span class="text-gray-400">GN Drive Version</span>
                            <span class="text-gray-300">1.0.0</span>
                        </div>
                        <div class="flex justify-between text-sm">
                            <span class="text-gray-400">Powered by</span>
                            <span class="text-gray-300">rclone + Wails v3</span>
                        </div>
                    </div>
                </p-card>
            </div>
        </div>

        <!-- Export Dialog -->
        <p-dialog
            header="Export Configuration"
            [(visible)]="showExportDialog"
            [modal]="true"
            [style]="{ width: '400px' }"
            [closable]="!isExporting"
        >
            <div class="space-y-4">
                <p class="text-sm text-gray-400">
                    Select what to include in the backup:
                </p>

                <div class="space-y-3">
                    <div class="flex items-center gap-2">
                        <p-checkbox
                            [(ngModel)]="exportOptions.include_boards"
                            [binary]="true"
                            inputId="exportBoards"
                        ></p-checkbox>
                        <label for="exportBoards" class="text-sm text-gray-300">
                            Boards ({{ exportPreview?.board_count || 0 }})
                        </label>
                    </div>
                    <div class="flex items-center gap-2">
                        <p-checkbox
                            [(ngModel)]="exportOptions.include_remotes"
                            [binary]="true"
                            inputId="exportRemotes"
                        ></p-checkbox>
                        <label
                            for="exportRemotes"
                            class="text-sm text-gray-300"
                        >
                            Remotes ({{ exportPreview?.remote_count || 0 }})
                        </label>
                    </div>
                </div>

                @if (exportOptions.include_remotes) {
                    <div
                        class="p-3 bg-yellow-900/30 border border-yellow-700/50 rounded text-xs text-yellow-300"
                    >
                        <i class="pi pi-exclamation-triangle mr-2"></i>
                        Remotes contain authentication tokens. Keep the backup
                        file secure.
                    </div>

                    <div class="flex items-center gap-2">
                        <p-checkbox
                            [(ngModel)]="exportOptions.exclude_tokens"
                            [binary]="true"
                            inputId="excludeTokens"
                        ></p-checkbox>
                        <label
                            for="excludeTokens"
                            class="text-sm text-gray-400"
                        >
                            Exclude authentication tokens (requires
                            re-authentication after import)
                        </label>
                    </div>
                }
            </div>

            <ng-template #footer>
                <div class="flex justify-end gap-2">
                    <p-button
                        label="Cancel"
                        severity="secondary"
                        [text]="true"
                        [disabled]="isExporting"
                        (onClick)="showExportDialog = false"
                    ></p-button>
                    <p-button
                        label="Export"
                        icon="pi pi-upload"
                        [loading]="isExporting"
                        [disabled]="
                            !exportOptions.include_boards &&
                            !exportOptions.include_remotes
                        "
                        (onClick)="doExport()"
                    ></p-button>
                </div>
            </ng-template>
        </p-dialog>

        <!-- Import Dialog -->
        <p-dialog
            header="Import Configuration"
            [(visible)]="showImportDialog"
            [modal]="true"
            [style]="{ width: '450px' }"
            [closable]="!isImporting"
        >
            @if (!importPreview) {
                <div class="text-center py-4">
                    <p class="text-sm text-gray-400 mb-4">
                        Select a backup file (.nsd) to import
                    </p>
                    <p-button
                        label="Select File"
                        icon="pi pi-folder-open"
                        [loading]="isLoadingPreview"
                        (onClick)="selectImportFile()"
                    ></p-button>
                </div>
            } @else {
                <div class="space-y-4">
                    <!-- Manifest Info -->
                    <div class="p-3 bg-surface-800 rounded text-xs space-y-1">
                        <div class="flex justify-between">
                            <span class="text-gray-400">Backup Date:</span>
                            <span class="text-gray-300">{{
                                importPreview.manifest?.export_date
                                    | date: "medium"
                            }}</span>
                        </div>
                        <div class="flex justify-between">
                            <span class="text-gray-400">File:</span>
                            <span
                                class="text-gray-300 truncate max-w-[200px]"
                                [title]="importFilePath"
                                >{{ importFileName }}</span
                            >
                        </div>
                    </div>

                    <!-- Preview Sections -->
                    @if (importPreview.boards) {
                        <div class="border border-surface-600 rounded p-3">
                            <h4
                                class="text-sm font-medium text-gray-300 mb-2 flex items-center gap-2"
                            >
                                <i class="pi pi-sitemap text-xs"></i>
                                Boards ({{ importPreview.boards.total }})
                            </h4>
                            <div class="text-xs space-y-1">
                                @if (importPreview.boards.to_add.length) {
                                    <div class="text-green-400">
                                        +
                                        {{ importPreview.boards.to_add.length }}
                                        to add:
                                        {{
                                            importPreview.boards.to_add.join(
                                                ", "
                                            )
                                        }}
                                    </div>
                                }
                                @if (importPreview.boards.to_update.length) {
                                    <div class="text-yellow-400">
                                        ~
                                        {{
                                            importPreview.boards.to_update
                                                .length
                                        }}
                                        existing:
                                        {{
                                            importPreview.boards.to_update.join(
                                                ", "
                                            )
                                        }}
                                    </div>
                                }
                            </div>
                        </div>
                    }

                    @if (importPreview.remotes) {
                        <div class="border border-surface-600 rounded p-3">
                            <h4
                                class="text-sm font-medium text-gray-300 mb-2 flex items-center gap-2"
                            >
                                <i class="pi pi-cloud text-xs"></i>
                                Remotes ({{ importPreview.remotes.total }})
                            </h4>
                            <div class="text-xs space-y-1">
                                @if (importPreview.remotes.to_add.length) {
                                    <div class="text-green-400">
                                        +
                                        {{
                                            importPreview.remotes.to_add.length
                                        }}
                                        to add:
                                        {{
                                            importPreview.remotes.to_add.join(
                                                ", "
                                            )
                                        }}
                                    </div>
                                }
                                @if (importPreview.remotes.to_update.length) {
                                    <div class="text-yellow-400">
                                        ~
                                        {{
                                            importPreview.remotes.to_update
                                                .length
                                        }}
                                        existing:
                                        {{
                                            importPreview.remotes.to_update.join(
                                                ", "
                                            )
                                        }}
                                    </div>
                                }
                            </div>
                        </div>
                    }

                    <!-- Warnings -->
                    @if (importPreview.warnings.length) {
                        <div
                            class="p-3 bg-yellow-900/30 border border-yellow-700/50 rounded text-xs text-yellow-300"
                        >
                            <div class="font-medium mb-1">
                                <i class="pi pi-exclamation-triangle mr-1"></i>
                                Warnings:
                            </div>
                            @for (
                                warning of importPreview.warnings;
                                track warning
                            ) {
                                <div>{{ warning }}</div>
                            }
                        </div>
                    }

                    <!-- Import Options -->
                    <div class="space-y-2">
                        <div class="flex items-center gap-2">
                            <p-checkbox
                                [(ngModel)]="importOptions.overwrite_boards"
                                [binary]="true"
                                inputId="overwriteBoards"
                            ></p-checkbox>
                            <label
                                for="overwriteBoards"
                                class="text-sm text-gray-400"
                            >
                                Overwrite existing boards
                            </label>
                        </div>
                        <div class="flex items-center gap-2">
                            <p-checkbox
                                [(ngModel)]="importOptions.overwrite_remotes"
                                [binary]="true"
                                inputId="overwriteRemotes"
                            ></p-checkbox>
                            <label
                                for="overwriteRemotes"
                                class="text-sm text-gray-400"
                            >
                                Overwrite existing remotes
                            </label>
                        </div>
                    </div>
                </div>
            }

            <ng-template #footer>
                <div class="flex justify-end gap-2">
                    <p-button
                        label="Cancel"
                        severity="secondary"
                        [text]="true"
                        [disabled]="isImporting"
                        (onClick)="closeImportDialog()"
                    ></p-button>
                    @if (importPreview) {
                        <p-button
                            label="Import"
                            icon="pi pi-download"
                            [loading]="isImporting"
                            (onClick)="doImport()"
                        ></p-button>
                    }
                </div>
            </ng-template>
        </p-dialog>
    `,
})
export class SettingsComponent implements OnInit {
    private readonly cdr = inject(ChangeDetectorRef);
    private readonly messageService = inject(MessageService);

    notificationsEnabled = true;
    minimizeToTray = false;
    startAtLogin = false;
    configEncrypted = false;

    // Export
    showExportDialog = false;
    isExporting = false;
    exportOptions: ExportOptions = {
        include_boards: true,
        include_remotes: true,
        include_settings: false,
        exclude_tokens: false,
    };
    exportPreview: { board_count: number; remote_count: number } | null = null;

    // Import
    showImportDialog = false;
    isImporting = false;
    isLoadingPreview = false;
    importPreview: ImportPreview | null = null;
    importFilePath = "";
    importOptions: ImportOptions = {
        overwrite_boards: false,
        overwrite_remotes: false,
        merge_mode: false,
    };

    get importFileName(): string {
        if (!this.importFilePath) return "";
        return this.importFilePath.split("/").pop() || this.importFilePath;
    }

    async ngOnInit() {
        try {
            const settings = await GetSettings();
            this.notificationsEnabled = settings.notifications_enabled;
            this.minimizeToTray = settings.minimize_to_tray;
            this.startAtLogin = settings.start_at_login;
            this.cdr.markForCheck();
        } catch (e) {
            console.error("Failed to load settings:", e);
        }
    }

    async saveNotificationSetting() {
        try {
            await SetEnabled(this.notificationsEnabled);
        } catch (e) {
            console.error("Failed to save notification setting:", e);
        }
    }

    async saveMinimizeToTraySetting() {
        try {
            await SetMinimizeToTray(this.minimizeToTray);
        } catch (e) {
            console.error("Failed to save minimize to tray setting:", e);
        }
    }

    async saveStartAtLoginSetting() {
        try {
            await SetStartAtLogin(this.startAtLogin);
        } catch (e) {
            console.error("Failed to save start at login setting:", e);
            // Revert the toggle on error
            this.startAtLogin = !this.startAtLogin;
            this.cdr.markForCheck();
            this.messageService.add({
                severity: "error",
                summary: "Error",
                detail: "Failed to update start at login setting",
            });
        }
    }

    toggleConfigEncryption() {
        // Config encryption requires password dialog — not yet implemented
    }

    // Export
    async openExportDialog() {
        this.showExportDialog = true;
        try {
            const preview = await GetExportPreview(this.exportOptions);
            if (preview) {
                this.exportPreview = {
                    board_count: preview.board_count,
                    remote_count: preview.remote_count,
                };
            }
            this.cdr.markForCheck();
        } catch (e) {
            console.error("Failed to get export preview:", e);
        }
    }

    async doExport() {
        this.isExporting = true;
        this.cdr.markForCheck();

        try {
            // First select file
            const filePath = await SelectExportFile();
            if (!filePath) {
                // User cancelled
                this.isExporting = false;
                this.cdr.markForCheck();
                return;
            }

            // Then export to the selected file
            await ExportToFile(filePath, this.exportOptions);
            this.messageService.add({
                severity: "success",
                summary: "Export Complete",
                detail: `Backup saved to ${filePath}`,
            });
            this.showExportDialog = false;
        } catch (e) {
            this.messageService.add({
                severity: "error",
                summary: "Export Failed",
                detail: String(e),
            });
        } finally {
            this.isExporting = false;
            this.cdr.markForCheck();
        }
    }

    // Import
    openImportDialog() {
        this.showImportDialog = true;
        this.importPreview = null;
        this.importFilePath = "";
        this.importOptions = {
            overwrite_boards: false,
            overwrite_remotes: false,
            merge_mode: false,
        };
    }

    closeImportDialog() {
        this.showImportDialog = false;
        this.importPreview = null;
        this.importFilePath = "";
    }

    async selectImportFile() {
        this.isLoadingPreview = true;
        this.cdr.markForCheck();

        try {
            // First select file
            const filePath = await SelectImportFile();
            if (!filePath) {
                // User cancelled
                this.isLoadingPreview = false;
                this.cdr.markForCheck();
                return;
            }

            // Then validate and get preview
            const preview = await ValidateImportFile(filePath);
            if (preview) {
                this.importPreview = preview;
                this.importFilePath = filePath;
            }
        } catch (e) {
            this.messageService.add({
                severity: "error",
                summary: "Invalid File",
                detail: String(e),
            });
        } finally {
            this.isLoadingPreview = false;
            this.cdr.markForCheck();
        }
    }

    async doImport() {
        if (!this.importFilePath) return;

        this.isImporting = true;
        this.cdr.markForCheck();

        try {
            const result = await ImportFromFile(
                this.importFilePath,
                this.importOptions,
            );
            if (result) {
                const details: string[] = [];
                if (result.boards_added > 0)
                    details.push(`${result.boards_added} boards added`);
                if (result.boards_updated > 0)
                    details.push(`${result.boards_updated} boards updated`);
                if (result.remotes_added > 0)
                    details.push(`${result.remotes_added} remotes added`);
                if (result.remotes_updated > 0)
                    details.push(`${result.remotes_updated} remotes updated`);

                this.messageService.add({
                    severity: result.success ? "success" : "warn",
                    summary: result.success
                        ? "Import Complete"
                        : "Import Completed with Warnings",
                    detail: details.join(", ") || "No changes made",
                });

                if (result.warnings?.length) {
                    for (const warning of result.warnings) {
                        this.messageService.add({
                            severity: "warn",
                            summary: "Warning",
                            detail: warning,
                        });
                    }
                }

                this.closeImportDialog();
            }
        } catch (e) {
            this.messageService.add({
                severity: "error",
                summary: "Import Failed",
                detail: String(e),
            });
        } finally {
            this.isImporting = false;
            this.cdr.markForCheck();
        }
    }
}
