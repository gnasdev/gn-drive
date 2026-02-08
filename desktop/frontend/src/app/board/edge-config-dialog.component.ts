import { CommonModule } from "@angular/common";
import {
    ChangeDetectionStrategy,
    Component,
    EventEmitter,
    Input,
    Output,
} from "@angular/core";
import { FormsModule } from "@angular/forms";
import { ButtonModule } from "primeng/button";
import { Card } from "primeng/card";
import { Dialog } from "primeng/dialog";
import { InputNumber } from "primeng/inputnumber";
import { InputText } from "primeng/inputtext";
import { Select } from "primeng/select";
import { ToggleSwitch } from "primeng/toggleswitch";
import * as models from "../../../wailsjs/desktop/backend/models/models.js";
import { REMOTE_TYPE_OPTIONS } from "../remotes/remotes.types.js";

@Component({
    selector: "app-edge-config-dialog",
    standalone: true,
    imports: [
        CommonModule,
        FormsModule,
        Dialog,
        ButtonModule,
        Card,
        InputText,
        InputNumber,
        Select,
        ToggleSwitch,
    ],
    templateUrl: "./edge-config-dialog.component.html",
    changeDetection: ChangeDetectionStrategy.OnPush,
})
export class EdgeConfigDialogComponent {
    @Input() visible = false;
    @Input() edge: models.BoardEdge | null = null;
    @Input() sourceLabel = "";
    @Input() targetLabel = "";
    @Input() sourceProvider = "";
    @Input() targetProvider = "";
    @Input() sourceType = "";
    @Input() targetType = "";

    @Output() visibleChange = new EventEmitter<boolean>();
    @Output() edgeSaved = new EventEmitter<models.BoardEdge>();
    @Output() edgeDeleted = new EventEmitter<string>();

    activeTab: "general" | "filters" | "performance" | "advanced" = "general";

    readonly tabs = [
        { id: "general" as const, label: "General", icon: "pi pi-cog" },
        { id: "filters" as const, label: "Filters", icon: "pi pi-filter" },
        {
            id: "performance" as const,
            label: "Performance",
            icon: "pi pi-bolt",
        },
        { id: "advanced" as const, label: "Advanced", icon: "pi pi-sliders-h" },
    ];

    readonly conflictResolutionOptions = [
        { value: "", label: "Default" },
        { value: "newer", label: "Newer wins" },
        { value: "older", label: "Older wins" },
        { value: "larger", label: "Larger wins" },
        { value: "smaller", label: "Smaller wins" },
        { value: "path1", label: "Path 1 wins" },
        { value: "path2", label: "Path 2 wins" },
    ];

    onDelete(): void {
        if (this.edge) {
            this.edgeDeleted.emit(this.edge.id);
        }
        this.visible = false;
        this.visibleChange.emit(false);
    }

    onClose(): void {
        // Auto-save on close
        if (this.edge) {
            this.edgeSaved.emit(this.edge);
        }
        this.visible = false;
        this.visibleChange.emit(false);
        this.activeTab = "general";
    }

    isBidirectional(): boolean {
        return this.edge?.action === "bi" || this.edge?.action === "bi-resync";
    }

    getProviderIcon(type: string): string {
        const option = REMOTE_TYPE_OPTIONS.find((opt) => opt.value === type);
        return option?.icon ?? "";
    }

    onIconError(event: Event): void {
        const img = event.target as HTMLElement;
        img.style.display = "none";
        const fallback = img.nextElementSibling as HTMLElement | null;
        if (fallback) {
            fallback.style.display = "inline";
        }
    }
}
