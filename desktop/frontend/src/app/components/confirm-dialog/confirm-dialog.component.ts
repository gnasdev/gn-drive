import { Component, EventEmitter, Input, Output } from "@angular/core";
import { CommonModule } from "@angular/common";
import { ButtonModule } from "primeng/button";
import { Dialog } from "primeng/dialog";

export interface ConfirmDialogData {
  title: string;
  message: string;
  confirmText?: string;
  cancelText?: string;
  confirmSeverity?: string;
  warning?: string;
}

@Component({
  selector: "app-confirm-dialog",
  standalone: true,
  imports: [CommonModule, ButtonModule, Dialog],
  template: `
    <p-dialog
      [header]="data.title"
      [(visible)]="isOpen"
      [modal]="true"
      [draggable]="false"
      [closable]="true"
      [style]="{ width: '24rem' }"
      (onHide)="onCancel()"
    >
      <div class="mb-4">
        <p class="text-gray-300">{{ data.message }}</p>
        @if (data.warning) {
        <div
          class="bg-yellow-900/20 border border-yellow-800 rounded-lg p-3 mt-3"
        >
          <p class="text-yellow-200 text-sm">
            <strong>Warning:</strong> {{ data.warning }}
          </p>
        </div>
        }
      </div>
      <div class="flex justify-end gap-3">
        <p-button
          [label]="data.cancelText || 'Cancel'"
          severity="secondary"
          (onClick)="onCancel()"
        />
        <p-button
          [label]="data.confirmText || 'Confirm'"
          [severity]="(data.confirmSeverity as any) || 'primary'"
          (onClick)="onConfirm()"
        />
      </div>
    </p-dialog>
  `,
})
export class ConfirmDialogComponent {
  @Input() isOpen = false;
  @Input() data: ConfirmDialogData = {
    title: "Confirm",
    message: "Are you sure?",
  };

  @Output() confirmed = new EventEmitter<void>();
  @Output() cancelled = new EventEmitter<void>();

  onConfirm(): void {
    this.confirmed.emit();
  }

  onCancel(): void {
    this.cancelled.emit();
  }
}
