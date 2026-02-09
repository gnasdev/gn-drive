import {
  Component,
  OnInit,
  OnDestroy,
  inject,
  ChangeDetectorRef,
  ChangeDetectionStrategy,
} from "@angular/core";
import { CommonModule } from "@angular/common";
import { Subscription } from "rxjs";
import {
  ErrorService,
  ErrorSeverity,
} from "../../services/error.service";
import { MessageService } from "primeng/api";
import {
  trigger,
  transition,
  style,
  animate,
} from "@angular/animations";

interface ToastItem {
  id: string;
  severity: "success" | "info" | "warn" | "error";
  summary: string;
  detail: string;
  life: number;
  removing?: boolean;
}

@Component({
  selector: "app-toast",
  standalone: true,
  imports: [CommonModule],
  changeDetection: ChangeDetectionStrategy.OnPush,
  animations: [
    trigger("toastAnim", [
      transition(":enter", [
        style({ opacity: 0, transform: "translateX(100%)" }),
        animate(
          "200ms ease-out",
          style({ opacity: 1, transform: "translateX(0)" })
        ),
      ]),
      transition(":leave", [
        animate(
          "150ms ease-in",
          style({ opacity: 0, transform: "translateX(100%)" })
        ),
      ]),
    ]),
  ],
  template: `
    <div class="fixed top-4 right-4 z-[9999] flex flex-col gap-3 max-w-sm w-full pointer-events-none">
      @for (toast of toasts; track toast.id) {
        <div
          @toastAnim
          class="pointer-events-auto border-2 border-black shadow-[4px_4px_0px_0px_rgba(0,0,0,1)] p-4 flex items-start gap-3 cursor-pointer transition-transform hover:translate-x-[-2px] hover:translate-y-[-2px] hover:shadow-[6px_6px_0px_0px_rgba(0,0,0,1)]"
          [ngClass]="getSeverityClasses(toast.severity)"
          (click)="removeToast(toast.id)"
        >
          <!-- Icon -->
          <div class="flex-shrink-0 text-lg font-bold mt-0.5">
            @switch (toast.severity) {
              @case ('success') { <span>&#10003;</span> }
              @case ('info') { <span>i</span> }
              @case ('warn') { <span>!</span> }
              @case ('error') { <span>&#10005;</span> }
            }
          </div>

          <!-- Content -->
          <div class="flex-1 min-w-0">
            <div class="font-bold text-sm leading-tight">{{ toast.summary }}</div>
            @if (toast.detail) {
              <div class="text-sm mt-1 opacity-90 leading-snug break-words">{{ toast.detail }}</div>
            }
          </div>

          <!-- Close -->
          <button
            class="flex-shrink-0 font-bold text-base leading-none opacity-70 hover:opacity-100 transition-opacity"
            (click)="removeToast(toast.id); $event.stopPropagation()"
          >&times;</button>
        </div>
      }
    </div>
  `,
})
export class ToastComponent implements OnInit, OnDestroy {
  private readonly errorService = inject(ErrorService);
  private readonly messageService = inject(MessageService);
  private readonly cdr = inject(ChangeDetectorRef);

  private subscriptions = new Subscription();
  private shownErrorIds = new Set<string>();
  private toastCounter = 0;
  private timers = new Map<string, ReturnType<typeof setTimeout>>();

  toasts: ToastItem[] = [];

  ngOnInit(): void {
    // Bridge: ErrorService → MessageService (keep existing behavior)
    this.subscriptions.add(
      this.errorService.errors$.subscribe((errors) => {
        const toastNotifications = errors.filter(
          (error) => !error.dismissed && error.autoHide
        );

        for (const notification of toastNotifications) {
          if (this.shownErrorIds.has(notification.id)) continue;
          this.shownErrorIds.add(notification.id);

          this.messageService.add({
            severity: this.mapSeverity(notification.severity),
            summary: notification.title,
            detail: notification.message,
            life: notification.duration || 3000,
          });

          if (notification.duration && notification.duration > 0) {
            setTimeout(() => {
              this.errorService.dismissError(notification.id);
            }, notification.duration);
          }
        }
      })
    );

    // Render: Listen to MessageService for all toast messages
    this.subscriptions.add(
      this.messageService.messageObserver.subscribe((msg) => {
        if (Array.isArray(msg)) {
          for (const m of msg) this.addToast(m);
        } else {
          this.addToast(msg);
        }
      })
    );

    // Listen for clear events
    this.subscriptions.add(
      this.messageService.clearObserver.subscribe(() => {
        this.clearAll();
      })
    );
  }

  ngOnDestroy(): void {
    this.subscriptions.unsubscribe();
    for (const timer of this.timers.values()) {
      clearTimeout(timer);
    }
  }

  getSeverityClasses(severity: string): Record<string, boolean> {
    return {
      "bg-green-100 text-green-900 dark:bg-green-900 dark:text-green-100": severity === "success",
      "bg-blue-100 text-blue-900 dark:bg-blue-900 dark:text-blue-100": severity === "info",
      "bg-amber-100 text-amber-900 dark:bg-amber-900 dark:text-amber-100": severity === "warn",
      "bg-red-100 text-red-900 dark:bg-red-900 dark:text-red-100": severity === "error",
    };
  }

  removeToast(id: string): void {
    const timer = this.timers.get(id);
    if (timer) {
      clearTimeout(timer);
      this.timers.delete(id);
    }
    this.toasts = this.toasts.filter((t) => t.id !== id);
    this.cdr.markForCheck();
  }

  private addToast(msg: { severity?: string; summary?: string; detail?: string; life?: number }): void {
    const id = `toast_${++this.toastCounter}_${Date.now()}`;
    const life = msg.life || 3000;

    const toast: ToastItem = {
      id,
      severity: (msg.severity as ToastItem["severity"]) || "info",
      summary: msg.summary || "",
      detail: msg.detail || "",
      life,
    };

    this.toasts = [...this.toasts, toast];
    this.cdr.markForCheck();

    if (life > 0) {
      const timer = setTimeout(() => {
        this.removeToast(id);
      }, life);
      this.timers.set(id, timer);
    }
  }

  private clearAll(): void {
    for (const timer of this.timers.values()) {
      clearTimeout(timer);
    }
    this.timers.clear();
    this.toasts = [];
    this.cdr.markForCheck();
  }

  private mapSeverity(
    severity: ErrorSeverity
  ): "success" | "info" | "warn" | "error" {
    switch (severity) {
      case ErrorSeverity.INFO:
        return "info";
      case ErrorSeverity.WARNING:
        return "warn";
      case ErrorSeverity.ERROR:
        return "error";
      case ErrorSeverity.CRITICAL:
        return "error";
      default:
        return "info";
    }
  }
}
