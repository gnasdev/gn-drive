import { ErrorHandler, Injectable, NgZone } from "@angular/core";
import { ErrorService } from "./error.service";

@Injectable()
export class GlobalErrorHandler implements ErrorHandler {
  constructor(private errorService: ErrorService, private zone: NgZone) {}

  handleError(error: unknown): void {
    // Log to console for debugging
    console.error("Global error caught:", error);

    // Run inside Angular zone to ensure change detection works
    this.zone.run(() => {
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
    });
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
    this.errorService.showWarning(
      "Application update detected. Please refresh the page.",
      10000
    );
  }
}
