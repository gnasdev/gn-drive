import { Component, OnInit, OnDestroy, inject } from "@angular/core";
import { Subscription } from "rxjs";
import {
  ErrorService,
  ErrorSeverity,
} from "../../services/error.service";
import { MessageService } from "primeng/api";

/**
 * Toast bridge: subscribes to ErrorService and forwards notifications
 * to PrimeNG MessageService. The <p-toast> in app.component.html renders them.
 */
@Component({
  selector: "app-toast",
  standalone: true,
  template: "",
})
export class ToastComponent implements OnInit, OnDestroy {
  private readonly errorService = inject(ErrorService);
  private readonly messageService = inject(MessageService);

  private subscription?: Subscription;
  private shownIds = new Set<string>();

  ngOnInit(): void {
    this.subscription = this.errorService.errors$.subscribe((errors) => {
      const toastNotifications = errors.filter(
        (error) => !error.dismissed && error.autoHide
      );

      for (const notification of toastNotifications) {
        if (this.shownIds.has(notification.id)) continue;
        this.shownIds.add(notification.id);

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
    });
  }

  ngOnDestroy(): void {
    this.subscription?.unsubscribe();
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
