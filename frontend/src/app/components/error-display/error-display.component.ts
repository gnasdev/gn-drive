import {
  Component,
  OnInit,
  OnDestroy,
  ChangeDetectionStrategy,
  ChangeDetectorRef,
} from "@angular/core";
import { CommonModule } from "@angular/common";
import { Subscription } from "rxjs";
import {
  ErrorService,
  ErrorNotification,
  ErrorSeverity,
} from "../../services/error.service";
import {
  LucideAngularModule,
  AlertCircle,
  AlertTriangle,
  Info,
  X,
  ChevronDown,
  ChevronUp,
} from "lucide-angular";

@Component({
  selector: "app-error-display",
  standalone: true,
  imports: [CommonModule, LucideAngularModule],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div
      class="fixed top-20 right-4 w-96 max-h-[calc(100vh-100px)] overflow-y-auto z-50 space-y-2"
      *ngIf="activeErrors.length > 0"
    >
      <div
        *ngFor="let error of activeErrors; trackBy: trackByErrorId"
        [class]="getErrorCardClasses(error.severity)"
        class="bg-white dark:bg-gray-800 rounded-lg shadow-lg border-l-4 p-4 animate-slide-in"
      >
        <!-- Header -->
        <div class="flex items-start justify-between mb-3">
          <div class="flex items-center space-x-3">
            <div
              [class]="getIconClasses(error.severity)"
              class="flex-shrink-0 w-8 h-8 rounded-full flex items-center justify-center"
            >
              <lucide-icon
                [img]="getErrorIcon(error.severity)"
                [size]="16"
              ></lucide-icon>
            </div>
            <div class="flex-1 min-w-0">
              <h4
                class="text-sm font-semibold text-gray-900 dark:text-gray-100"
              >
                {{ error.title }}
              </h4>
              <div class="flex items-center space-x-2 mt-1">
                <span class="text-xs text-gray-500 dark:text-gray-400">{{
                  error.timestamp | date : "short"
                }}</span>
                <span
                  [class]="getSeverityChipClasses(error.severity)"
                  class="px-2 py-1 text-xs font-medium rounded-full"
                >
                  {{ error.severity.toUpperCase() }}
                </span>
              </div>
            </div>
          </div>
          <button
            (click)="dismissError(error.id)"
            class="flex-shrink-0 ml-2 text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 transition-colors"
            aria-label="Dismiss error"
          >
            <lucide-icon [img]="XIcon" [size]="16"></lucide-icon>
          </button>
        </div>

        <!-- Message -->
        <p class="text-sm text-gray-700 dark:text-gray-300 mb-3">
          {{ error.message }}
        </p>

        <!-- Details (expandable) -->
        <div
          *ngIf="error.details"
          class="border-t border-gray-200 dark:border-gray-600 pt-3"
        >
          <button
            (click)="toggleDetails(error.id)"
            class="flex items-center space-x-2 text-sm text-gray-600 dark:text-gray-400 hover:text-gray-800 dark:hover:text-gray-200 transition-colors"
          >
            <span>Details</span>
            <lucide-icon
              [img]="
                isDetailsExpanded(error.id) ? ChevronUpIcon : ChevronDownIcon
              "
              [size]="16"
            >
            </lucide-icon>
          </button>
          <div
            *ngIf="isDetailsExpanded(error.id)"
            class="mt-2 p-3 bg-gray-50 dark:bg-gray-700 rounded text-xs font-mono text-gray-600 dark:text-gray-300 whitespace-pre-wrap max-h-32 overflow-y-auto"
          >
            {{ error.details }}
          </div>
        </div>
      </div>
    </div>
  `,
  styles: [
    `
      .animate-slide-in {
        animation: slideIn 0.3s ease-out;
      }

      @keyframes slideIn {
        from {
          transform: translateX(100%);
          opacity: 0;
        }
        to {
          transform: translateX(0);
          opacity: 1;
        }
      }

      @media (max-width: 768px) {
        .fixed.top-20.right-4.w-96 {
          width: calc(100vw - 2rem);
          right: 1rem;
          left: 1rem;
        }
      }
    `,
  ],
})
export class ErrorDisplayComponent implements OnInit, OnDestroy {
  activeErrors: ErrorNotification[] = [];
  private subscription?: Subscription;
  private expandedDetails = new Set<string>();

  // Lucide icons
  AlertCircleIcon = AlertCircle;
  AlertTriangleIcon = AlertTriangle;
  InfoIcon = Info;
  XIcon = X;
  ChevronDownIcon = ChevronDown;
  ChevronUpIcon = ChevronUp;

  constructor(
    private errorService: ErrorService,
    private cdr: ChangeDetectorRef
  ) {}

  ngOnInit(): void {
    this.subscription = this.errorService.errors$.subscribe((errors) => {
      // Only show persistent errors (non-auto-hide) in the error display
      this.activeErrors = errors.filter(
        (error) => !error.dismissed && !error.autoHide
      );
      this.cdr.detectChanges();
    });
  }

  ngOnDestroy(): void {
    this.subscription?.unsubscribe();
  }

  dismissError(errorId: string): void {
    this.errorService.dismissError(errorId);
  }

  toggleDetails(errorId: string): void {
    if (this.expandedDetails.has(errorId)) {
      this.expandedDetails.delete(errorId);
    } else {
      this.expandedDetails.add(errorId);
    }
    this.cdr.detectChanges();
  }

  isDetailsExpanded(errorId: string): boolean {
    return this.expandedDetails.has(errorId);
  }

  getErrorIcon(severity: ErrorSeverity): typeof this.InfoIcon {
    switch (severity) {
      case ErrorSeverity.INFO:
        return this.InfoIcon;
      case ErrorSeverity.WARNING:
        return this.AlertTriangleIcon;
      case ErrorSeverity.ERROR:
      case ErrorSeverity.CRITICAL:
        return this.AlertCircleIcon;
      default:
        return this.AlertCircleIcon;
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
        return "border-l-purple-500 shadow-purple-200 dark:shadow-purple-900";
      default:
        return "border-l-red-500";
    }
  }

  getIconClasses(severity: ErrorSeverity): string {
    switch (severity) {
      case ErrorSeverity.INFO:
        return "bg-blue-100 text-blue-600 dark:bg-blue-900 dark:text-blue-300";
      case ErrorSeverity.WARNING:
        return "bg-yellow-100 text-yellow-600 dark:bg-yellow-900 dark:text-yellow-300";
      case ErrorSeverity.ERROR:
        return "bg-red-100 text-red-600 dark:bg-red-900 dark:text-red-300";
      case ErrorSeverity.CRITICAL:
        return "bg-purple-100 text-purple-600 dark:bg-purple-900 dark:text-purple-300";
      default:
        return "bg-red-100 text-red-600 dark:bg-red-900 dark:text-red-300";
    }
  }

  getSeverityChipClasses(severity: ErrorSeverity): string {
    switch (severity) {
      case ErrorSeverity.INFO:
        return "bg-blue-100 text-blue-800 dark:bg-blue-900 dark:text-blue-200";
      case ErrorSeverity.WARNING:
        return "bg-yellow-100 text-yellow-800 dark:bg-yellow-900 dark:text-yellow-200";
      case ErrorSeverity.ERROR:
        return "bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-200";
      case ErrorSeverity.CRITICAL:
        return "bg-purple-100 text-purple-800 dark:bg-purple-900 dark:text-purple-200";
      default:
        return "bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-200";
    }
  }

  trackByErrorId(_index: number, error: ErrorNotification): string {
    return error.id;
  }
}
