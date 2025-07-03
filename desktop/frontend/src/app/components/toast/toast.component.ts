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
import { LucideAngularModule, X } from "lucide-angular";

@Component({
  selector: "app-toast",
  standalone: true,
  imports: [CommonModule, LucideAngularModule],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="fixed top-4 right-4 z-50 space-y-2 w-80" *ngIf="toastNotifications.length > 0">
      <div 
        *ngFor="let notification of toastNotifications; trackBy: trackByNotificationId"
        [class]="getToastClasses(notification.severity)"
        class="flex items-center p-4 rounded-lg shadow-lg animate-slide-in">
        
        <div class="flex-1 min-w-0">
          <p class="text-sm font-medium text-white">{{ notification.title }}</p>
          <p class="text-sm text-white/90">{{ notification.message }}</p>
        </div>
        
        <button 
          (click)="dismissToast(notification.id)"
          class="ml-3 text-white/70 hover:text-white transition-colors"
          aria-label="Dismiss notification">
          <lucide-icon [img]="XIcon" [size]="16"></lucide-icon>
        </button>
      </div>
    </div>
  `,
  styles: [`
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
      .fixed.top-4.right-4.w-80 {
        width: calc(100vw - 2rem);
        right: 1rem;
        left: 1rem;
      }
    }
  `]
})
export class ToastComponent implements OnInit, OnDestroy {
  toastNotifications: ErrorNotification[] = [];
  private subscription?: Subscription;

  XIcon = X;

  constructor(
    private errorService: ErrorService,
    private cdr: ChangeDetectorRef
  ) {}

  ngOnInit(): void {
    this.subscription = this.errorService.errors$.subscribe(errors => {
      // Only show auto-hide notifications as toasts
      this.toastNotifications = errors.filter(
        error => !error.dismissed && error.autoHide
      );
      
      // Auto-hide toasts after their duration
      this.toastNotifications.forEach(notification => {
        if (notification.duration && notification.duration > 0) {
          setTimeout(() => {
            this.dismissToast(notification.id);
          }, notification.duration);
        }
      });
      
      this.cdr.detectChanges();
    });
  }

  ngOnDestroy(): void {
    this.subscription?.unsubscribe();
  }

  dismissToast(notificationId: string): void {
    this.errorService.dismissError(notificationId);
  }

  getToastClasses(severity: ErrorSeverity): string {
    switch (severity) {
      case ErrorSeverity.INFO:
        return 'bg-blue-500';
      case ErrorSeverity.WARNING:
        return 'bg-yellow-500';
      case ErrorSeverity.ERROR:
        return 'bg-red-500';
      case ErrorSeverity.CRITICAL:
        return 'bg-purple-500';
      default:
        return 'bg-gray-500';
    }
  }

  trackByNotificationId(_index: number, notification: ErrorNotification): string {
    return notification.id;
  }
}
