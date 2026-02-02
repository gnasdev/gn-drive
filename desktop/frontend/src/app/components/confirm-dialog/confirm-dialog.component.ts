import { Component, EventEmitter, Input, Output } from "@angular/core";
import { CommonModule } from "@angular/common";

export interface ConfirmDialogData {
  title: string;
  message: string;
  confirmText?: string;
  cancelText?: string;
  confirmButtonClass?: string;
  warning?: string;
}

@Component({
  selector: "app-confirm-dialog",
  standalone: true,
  imports: [CommonModule],
  template: `
    @if (isOpen) {
    <div
      class="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50"
      (click)="onCancel()"
      (keyup.escape)="onCancel()"
      tabindex="0"
      role="dialog"
      aria-modal="true"
      [attr.aria-labelledby]="'confirm-dialog-title'"
    >
      <div
        class="bg-white dark:bg-gray-800 rounded-lg p-6 w-96 max-w-md mx-4 shadow-xl"
        (click)="$event.stopPropagation()"
        (keydown)="$event.stopPropagation()"
        tabindex="-1"
        role="document"
      >
        <h2
          id="confirm-dialog-title"
          class="text-xl font-bold text-gray-900 dark:text-gray-100 mb-4"
        >
          {{ data.title }}
        </h2>
        <div class="mb-6">
          <p class="text-gray-700 dark:text-gray-300">
            {{ data.message }}
          </p>
          @if (data.warning) {
          <div
            class="bg-yellow-50 dark:bg-yellow-900/20 border border-yellow-200 dark:border-yellow-800 rounded-lg p-3 mt-3"
          >
            <p class="text-yellow-800 dark:text-yellow-200 text-sm">
              <strong>Warning:</strong> {{ data.warning }}
            </p>
          </div>
          }
        </div>
        <div class="flex justify-end space-x-3">
          <button
            type="button"
            class="btn-secondary"
            (click)="onCancel()"
            #cancelButton
          >
            {{ data.cancelText || "Cancel" }}
          </button>
          <button
            type="button"
            [class]="data.confirmButtonClass || 'btn-primary'"
            (click)="onConfirm()"
          >
            {{ data.confirmText || "Confirm" }}
          </button>
        </div>
      </div>
    </div>
    }
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
