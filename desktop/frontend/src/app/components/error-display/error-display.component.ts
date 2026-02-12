import { Component, OnInit, OnDestroy, ChangeDetectionStrategy, ChangeDetectorRef, inject } from "@angular/core";
import { CommonModule } from "@angular/common";
import { Subscription } from "rxjs";
import {
  ErrorService,
  ErrorNotification,
  ErrorSeverity,
} from "../../services/error.service";
import { Dialog } from "primeng/dialog";
import { ButtonModule } from "primeng/button";

@Component({
  selector: "app-error-display",
  standalone: true,
  imports: [CommonModule, Dialog, ButtonModule],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <p-dialog
      header="Error Details"
      [(visible)]="dialogVisible"
      [modal]="true"
      [dismissableMask]="true"
      [draggable]="false"
      [closable]="true"
      [style]="{ width: '36rem', maxHeight: '80vh' }"
      (onHide)="onDialogHide()"
    >
      @if (activeErrors.length > 0) {
      <div class="space-y-3 max-h-[60vh] overflow-y-auto pr-1">
        @for (error of activeErrors; track error.id) {
        <div
          [class]="getErrorCardClasses(error.severity)"
          class="bg-gray-800 rounded-lg border-l-4 p-4"
        >
          <!-- Header -->
          <div class="flex items-start justify-between mb-2">
            <div class="flex items-center gap-3">
              <div
                [class]="getIconClasses(error.severity)"
                class="shrink-0 w-8 h-8 rounded-full flex items-center justify-center"
              >
                <i [class]="getErrorIconClass(error.severity)"></i>
              </div>
              <div>
                <h4 class="text-sm font-semibold text-gray-100">
                  {{ error.title }}
                </h4>
                <div class="flex items-center gap-2 mt-1">
                  <span class="text-xs text-gray-400">{{
                    error.timestamp | date : "medium"
                  }}</span>
                  <span
                    [class]="getSeverityChipClasses(error.severity)"
                    class="px-2 py-0.5 text-xs font-medium rounded-full"
                  >
                    {{ error.severity.toUpperCase() }}
                  </span>
                </div>
              </div>
            </div>
          </div>

          <!-- Message -->
          <p class="text-sm text-gray-300 mb-2">
            {{ error.message }}
          </p>

          <!-- Details -->
          @if (error.details) {
          <div
            class="p-3 bg-gray-700 rounded text-xs font-mono text-gray-300 whitespace-pre-wrap max-h-48 overflow-y-auto mb-2"
          >
            {{ error.details }}
          </div>
          }

          <!-- Actions -->
          <div class="flex items-center gap-2 pt-1">
            <p-button
              icon="pi pi-copy"
              label="Copy"
              [text]="true"
              severity="secondary"
              size="small"
              (onClick)="copyError(error)"
            ></p-button>
            <p-button
              icon="pi pi-times"
              label="Dismiss"
              [text]="true"
              severity="danger"
              size="small"
              (onClick)="dismissError(error.id)"
            ></p-button>
          </div>
        </div>
        }
      </div>
      }

      <ng-template #footer>
        <div class="flex justify-end">
          <p-button
            label="Clear All"
            icon="pi pi-trash"
            severity="secondary"
            (onClick)="clearAll()"
          ></p-button>
        </div>
      </ng-template>
    </p-dialog>
  `,
})
export class ErrorDisplayComponent implements OnInit, OnDestroy {
  private errorService = inject(ErrorService);
  private cdr = inject(ChangeDetectorRef);

  activeErrors: ErrorNotification[] = [];
  dialogVisible = false;
  private subscription?: Subscription;
  private previousErrorCount = 0;

  ngOnInit(): void {
    this.subscription = this.errorService.errors$.subscribe((errors) => {
      this.activeErrors = errors.filter(
        (error) => !error.dismissed && !error.autoHide
      );

      // Auto-open dialog when new persistent errors arrive
      if (this.activeErrors.length > this.previousErrorCount) {
        this.dialogVisible = true;
      }

      // Auto-close when all errors are dismissed
      if (this.activeErrors.length === 0) {
        this.dialogVisible = false;
      }

      this.previousErrorCount = this.activeErrors.length;
      this.cdr.markForCheck();
    });
  }

  ngOnDestroy(): void {
    this.subscription?.unsubscribe();
  }

  onDialogHide(): void {
    this.dialogVisible = false;
  }

  dismissError(errorId: string): void {
    this.errorService.dismissError(errorId);
  }

  clearAll(): void {
    this.errorService.clearAllErrors();
    this.dialogVisible = false;
  }

  async copyError(error: ErrorNotification): Promise<void> {
    const text = [
      `[${error.severity.toUpperCase()}] ${error.title}`,
      `Time: ${error.timestamp.toISOString()}`,
      `Message: ${error.message}`,
      error.details ? `Details: ${error.details}` : "",
    ]
      .filter(Boolean)
      .join("\n");

    try {
      await navigator.clipboard.writeText(text);
    } catch {
      console.error("Failed to copy error to clipboard");
    }
  }

  getErrorIconClass(severity: ErrorSeverity): string {
    switch (severity) {
      case ErrorSeverity.INFO:
        return "pi pi-info-circle";
      case ErrorSeverity.WARNING:
        return "pi pi-exclamation-triangle";
      case ErrorSeverity.ERROR:
      case ErrorSeverity.CRITICAL:
        return "pi pi-exclamation-circle";
      default:
        return "pi pi-exclamation-circle";
    }
  }

  getErrorCardClasses(severity: ErrorSeverity): string {
    switch (severity) {
      case ErrorSeverity.INFO:
        return "border-l-blue-500";
      case ErrorSeverity.WARNING:
        return "border-l-yellow-500";
      case ErrorSeverity.ERROR:
        return "border-l-red-500";
      case ErrorSeverity.CRITICAL:
        return "border-l-purple-500";
      default:
        return "border-l-red-500";
    }
  }

  getIconClasses(severity: ErrorSeverity): string {
    switch (severity) {
      case ErrorSeverity.INFO:
        return "bg-blue-900 text-blue-300";
      case ErrorSeverity.WARNING:
        return "bg-yellow-900 text-yellow-300";
      case ErrorSeverity.ERROR:
        return "bg-red-900 text-red-300";
      case ErrorSeverity.CRITICAL:
        return "bg-purple-900 text-purple-300";
      default:
        return "bg-red-900 text-red-300";
    }
  }

  getSeverityChipClasses(severity: ErrorSeverity): string {
    switch (severity) {
      case ErrorSeverity.INFO:
        return "bg-blue-900 text-blue-200";
      case ErrorSeverity.WARNING:
        return "bg-yellow-900 text-yellow-200";
      case ErrorSeverity.ERROR:
        return "bg-red-900 text-red-200";
      case ErrorSeverity.CRITICAL:
        return "bg-purple-900 text-purple-200";
      default:
        return "bg-red-900 text-red-200";
    }
  }
}
