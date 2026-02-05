import { CommonModule } from "@angular/common";
import {
  ChangeDetectionStrategy,
  Component,
  EventEmitter,
  Input,
  Output,
} from "@angular/core";
import { FormsModule } from "@angular/forms";
import { Dialog } from "primeng/dialog";
import { ButtonModule } from "primeng/button";
import { Card } from "primeng/card";
import { InputText } from "primeng/inputtext";
import { InputNumber } from "primeng/inputnumber";
import { Select } from "primeng/select";
import { ToggleSwitch } from "primeng/toggleswitch";
import * as models from "../../../wailsjs/desktop/backend/models/models.js";
import { EDGE_ACTIONS } from "./board.types.js";

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

  readonly actionOptions = EDGE_ACTIONS.map((a) => ({
    value: a.value,
    label: a.label,
  }));

  readonly conflictResolutionOptions = [
    { value: "", label: "Default" },
    { value: "newer", label: "Newer wins" },
    { value: "older", label: "Older wins" },
    { value: "larger", label: "Larger wins" },
    { value: "smaller", label: "Smaller wins" },
    { value: "path1", label: "Path 1 wins" },
    { value: "path2", label: "Path 2 wins" },
  ];

  onSave(): void {
    if (this.edge) {
      this.edgeSaved.emit(this.edge);
    }
    this.visible = false;
    this.visibleChange.emit(false);
  }

  onDelete(): void {
    if (this.edge) {
      this.edgeDeleted.emit(this.edge.id);
    }
    this.visible = false;
    this.visibleChange.emit(false);
  }

  onClose(): void {
    this.visible = false;
    this.visibleChange.emit(false);
    this.activeTab = "general";
  }
}
