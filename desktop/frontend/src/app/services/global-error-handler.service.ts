import { ErrorHandler, Injectable, inject } from "@angular/core";
import { ErrorService } from "./error.service";
import { LoggingService } from "./logging.service";

@Injectable()
export class GlobalErrorHandler implements ErrorHandler {
  private errorService = inject(ErrorService);
  private loggingService = inject(LoggingService);


  handleError(error: unknown): void {
    // Log to console for debugging
    console.error("Global error caught:", error);

    // Log to backend immediately for critical errors
    this.loggingService.critical(
      `Global Error: ${this.extractErrorMessage(error)}`,
      "global_error_handler",
      this.getErrorDetails(error),
      error instanceof Error ? error : undefined
    );

    // Check if it's a known error type
    if (this.isChunkLoadError(error)) {
      this.handleChunkLoadError();
    } else if (this.isNetworkError(error)) {
      this.errorService.handleNetworkError();
    } else if (this.isTimeoutError(error)) {
      this.errorService.handleTimeoutError();
    } else {
      // Generic error handling
      this.errorService.handleApiError(error, "global_error_handler");
    }
  }

  private isChunkLoadError(error: unknown): boolean {
    const message =
      error && typeof error === "object" && "message" in error
        ? String((error as { message: unknown }).message)
        : "";
    return (
      message.includes("Loading chunk") || message.includes("Loading CSS chunk")
    );
  }

  private isNetworkError(error: unknown): boolean {
    const message =
      error && typeof error === "object" && "message" in error
        ? String((error as { message: unknown }).message)
        : "";
    return (
      message.includes("Network Error") ||
      message.includes("ERR_NETWORK") ||
      message.includes("ERR_INTERNET_DISCONNECTED")
    );
  }

  private isTimeoutError(error: unknown): boolean {
    const message =
      error && typeof error === "object" && "message" in error
        ? String((error as { message: unknown }).message)
        : "";
    return message.includes("timeout") || message.includes("Timeout");
  }

  private handleChunkLoadError(): void {
    // For chunk load errors, we might want to reload the page
    this.loggingService.warn(
      "Chunk load error detected",
      "chunk_load_error",
      "Lazy loading chunk failed to load"
    );

    this.errorService.showWarning(
      "Application update detected. Please refresh the page.",
      10000
    );
  }

  private extractErrorMessage(error: unknown): string {
    if (typeof error === "string") {
      return error;
    }
    if (error && typeof error === "object" && "message" in error) {
      return String((error as { message: unknown }).message);
    }
    return "Unknown error";
  }

  private getErrorDetails(error: unknown): string {
    try {
      return JSON.stringify(error, null, 2);
    } catch {
      return String(error);
    }
  }
}
