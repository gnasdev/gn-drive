import { Injectable, inject } from "@angular/core";
import { BehaviorSubject } from "rxjs";
import { LoggingService } from "./logging.service";

export interface AppError {
  code: string;
  message: string;
  details?: string;
  timestamp: string;
  trace_id: string;
}

export interface ErrorResponse {
  error: AppError;
}

export enum ErrorSeverity {
  INFO = "info",
  WARNING = "warning",
  ERROR = "error",
  CRITICAL = "critical",
}

export interface ErrorNotification {
  id: string;
  severity: ErrorSeverity;
  title: string;
  message: string;
  details?: string;
  timestamp: Date;
  dismissed: boolean;
  autoHide: boolean;
  duration?: number;
}

@Injectable({
  providedIn: "root",
})
export class ErrorService {
  private loggingService = inject(LoggingService);

  private errorsSubject = new BehaviorSubject<ErrorNotification[]>([]);
  public errors$ = this.errorsSubject.asObservable();

  private errorCounter = 0;

  /**
   * Handle API errors from backend
   */
  handleApiError(error: unknown, context?: string): void {
    console.error("API Error:", error, "Context:", context);

    // Log to backend
    this.loggingService.error(
      `API Error: ${this.extractErrorMessage(error)}`,
      context || "api_error",
      JSON.stringify(error),
      error instanceof Error ? error : undefined
    );

    let errorNotification: ErrorNotification;

    if (this.isErrorResponse(error)) {
      // Structured error from backend
      errorNotification = this.createNotificationFromApiError(
        error.error,
        context
      );
    } else if (
      error &&
      typeof error === "object" &&
      "error" in error &&
      error.error &&
      typeof error.error === "object" &&
      "message" in error.error
    ) {
      // Standard HTTP error with message
      errorNotification = this.createNotification(
        ErrorSeverity.ERROR,
        "API Error",
        String((error.error as { message: unknown }).message),
        context
      );
    } else if (error && typeof error === "object" && "message" in error) {
      // JavaScript Error object
      errorNotification = this.createNotification(
        ErrorSeverity.ERROR,
        "Application Error",
        String((error as { message: unknown }).message),
        context
      );
    } else if (typeof error === "string") {
      // String error
      errorNotification = this.createNotification(
        ErrorSeverity.ERROR,
        "Error",
        error,
        context
      );
    } else {
      // Unknown error format
      errorNotification = this.createNotification(
        ErrorSeverity.ERROR,
        "Unknown Error",
        "An unexpected error occurred",
        context || JSON.stringify(error)
      );
    }

    this.addError(errorNotification);
  }

  /**
   * Handle validation errors
   */
  handleValidationError(field: string, message: string): void {
    // Log to backend
    this.loggingService.warn(
      `Validation Error: ${field}: ${message}`,
      "form_validation",
      `Field: ${field}, Message: ${message}`
    );

    const errorNotification = this.createNotification(
      ErrorSeverity.WARNING,
      "Validation Error",
      `${field}: ${message}`,
      "form_validation"
    );

    this.addError(errorNotification);
  }

  /**
   * Handle network errors
   */
  handleNetworkError(): void {
    // Log to backend
    this.loggingService.error(
      "Network Error: Unable to connect to the server",
      "network",
      "Connection failed - check network connectivity"
    );

    const errorNotification = this.createNotification(
      ErrorSeverity.ERROR,
      "Network Error",
      "Unable to connect to the server. Please check your connection.",
      "network"
    );

    this.addError(errorNotification);
  }

  /**
   * Handle timeout errors
   */
  handleTimeoutError(): void {
    const errorNotification = this.createNotification(
      ErrorSeverity.WARNING,
      "Request Timeout",
      "The request took too long to complete. Please try again.",
      "timeout"
    );

    this.addError(errorNotification);
  }

  /**
   * Show success message
   */
  showSuccess(message: string, duration = 3000): void {
    const notification = this.createNotification(
      ErrorSeverity.INFO,
      "Success",
      message
    );
    notification.autoHide = true;
    notification.duration = duration;
    this.addError(notification);
  }

  /**
   * Show info message
   */
  showInfo(message: string, duration = 5000): void {
    const notification = this.createNotification(
      ErrorSeverity.INFO,
      "Information",
      message
    );
    notification.autoHide = true;
    notification.duration = duration;
    this.addError(notification);
  }

  /**
   * Show warning message
   */
  showWarning(message: string, duration = 7000): void {
    const notification = this.createNotification(
      ErrorSeverity.WARNING,
      "Warning",
      message
    );
    notification.autoHide = true;
    notification.duration = duration;
    this.addError(notification);
  }

  /**
   * Dismiss an error notification
   */
  dismissError(errorId: string): void {
    const errors = this.errorsSubject.value;
    const updatedErrors = errors.map((error) =>
      error.id === errorId ? { ...error, dismissed: true } : error
    );
    this.errorsSubject.next(updatedErrors);
  }

  /**
   * Clear all errors
   */
  clearAllErrors(): void {
    this.errorsSubject.next([]);
  }

  /**
   * Get active (non-dismissed) errors
   */
  getActiveErrors(): ErrorNotification[] {
    return this.errorsSubject.value.filter((error) => !error.dismissed);
  }

  private isErrorResponse(error: unknown): error is ErrorResponse {
    return (
      error !== null &&
      typeof error === "object" &&
      error !== undefined &&
      "error" in error &&
      typeof (error as ErrorResponse).error === "object" &&
      (error as ErrorResponse).error !== null &&
      "code" in (error as ErrorResponse).error &&
      "message" in (error as ErrorResponse).error &&
      typeof (error as ErrorResponse).error.code === "string" &&
      typeof (error as ErrorResponse).error.message === "string"
    );
  }

  private createNotificationFromApiError(
    apiError: AppError,
    context?: string
  ): ErrorNotification {
    const severity = this.mapErrorCodeToSeverity(apiError.code);

    return {
      id: this.generateErrorId(),
      severity,
      title: this.getErrorTitle(apiError.code),
      message: apiError.message,
      details: apiError.details || context,
      timestamp: new Date(),
      dismissed: false,
      autoHide: severity !== ErrorSeverity.CRITICAL,
      duration: this.getErrorDuration(severity),
    };
  }

  private createNotification(
    severity: ErrorSeverity,
    title: string,
    message: string,
    details?: string
  ): ErrorNotification {
    return {
      id: this.generateErrorId(),
      severity,
      title,
      message,
      details,
      timestamp: new Date(),
      dismissed: false,
      autoHide: severity !== ErrorSeverity.CRITICAL,
      duration: this.getErrorDuration(severity),
    };
  }

  private addError(error: ErrorNotification): void {
    const errors = this.errorsSubject.value;
    this.errorsSubject.next([...errors, error]);
  }

  private mapErrorCodeToSeverity(code: string): ErrorSeverity {
    switch (code) {
      case "VALIDATION_ERROR":
      case "INVALID_INPUT":
      case "MISSING_FIELD":
        return ErrorSeverity.WARNING;

      case "NOT_FOUND_ERROR":
      case "TIMEOUT_ERROR":
        return ErrorSeverity.WARNING;

      case "AUTHENTICATION_ERROR":
      case "AUTHORIZATION_ERROR":
      case "NETWORK_ERROR":
        return ErrorSeverity.ERROR;

      case "INTERNAL_ERROR":
      case "DATABASE_ERROR":
      case "FILESYSTEM_ERROR":
        return ErrorSeverity.CRITICAL;

      default:
        return ErrorSeverity.ERROR;
    }
  }

  private getErrorTitle(code: string): string {
    switch (code) {
      case "VALIDATION_ERROR":
      case "INVALID_INPUT":
      case "MISSING_FIELD":
        return "Validation Error";

      case "AUTHENTICATION_ERROR":
        return "Authentication Error";

      case "AUTHORIZATION_ERROR":
        return "Access Denied";

      case "NOT_FOUND_ERROR":
        return "Not Found";

      case "NETWORK_ERROR":
        return "Network Error";

      case "TIMEOUT_ERROR":
        return "Request Timeout";

      case "RCLONE_ERROR":
        return "Sync Error";

      case "FILESYSTEM_ERROR":
        return "File System Error";

      case "INTERNAL_ERROR":
        return "System Error";

      default:
        return "Error";
    }
  }

  private getErrorDuration(severity: ErrorSeverity): number {
    switch (severity) {
      case ErrorSeverity.INFO:
        return 3000;
      case ErrorSeverity.WARNING:
        return 5000;
      case ErrorSeverity.ERROR:
        return 7000;
      case ErrorSeverity.CRITICAL:
        return 0; // Don't auto-hide critical errors
      default:
        return 5000;
    }
  }

  private generateErrorId(): string {
    return `error_${++this.errorCounter}_${Date.now()}`;
  }

  private extractErrorMessage(error: unknown): string {
    if (typeof error === "string") {
      return error;
    }
    if (error && typeof error === "object" && "message" in error) {
      return String((error as { message: unknown }).message);
    }
    if (this.isErrorResponse(error)) {
      return error.error.message;
    }
    return "Unknown error";
  }
}
