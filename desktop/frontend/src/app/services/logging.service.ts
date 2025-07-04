import { Injectable } from "@angular/core";
import { LogFrontendMessage } from "../../../bindings/desktop/backend/app";
import { FrontendLogEntry as BackendLogEntry } from "../../../bindings/desktop/backend/models/models";
import {
  LogLevel,
  FrontendLogEntry,
  BrowserInfo,
  LoggingConfig,
  QueuedLogEntry,
} from "../models/logging.interface";

@Injectable({
  providedIn: "root",
})
export class LoggingService {
  private config: LoggingConfig = {
    enabled: true,
    logLevel: LogLevel.WARN, // Only send WARN, ERROR, CRITICAL to backend
    maxRetries: 3,
    retryDelay: 1000,
    batchSize: 10,
    flushInterval: 5000,
    includeStackTrace: true,
    includeBrowserInfo: true,
    sanitizeUrls: true,
    maxLogEntrySize: 10000,
  };

  private logQueue: QueuedLogEntry[] = [];
  private isProcessingQueue = false;
  private flushTimer?: number;
  private logCounter = 0;

  constructor() {
    this.initializeLogging();
  }

  private initializeLogging(): void {
    // Start periodic queue processing
    this.startQueueProcessor();

    // Handle page unload to flush remaining logs
    window.addEventListener("beforeunload", () => {
      this.flushQueue();
    });

    // Handle visibility change to flush logs when page becomes hidden
    document.addEventListener("visibilitychange", () => {
      if (document.hidden) {
        this.flushQueue();
      }
    });
  }

  private startQueueProcessor(): void {
    this.flushTimer = window.setInterval(() => {
      this.processQueue();
    }, this.config.flushInterval);
  }

  private generateLogId(): string {
    return `log_${++this.logCounter}_${Date.now()}_${Math.random()
      .toString(36)
      .substr(2, 9)}`;
  }

  private generateTraceId(): string {
    return `trace_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
  }

  private getBrowserInfo(): BrowserInfo {
    return {
      userAgent: navigator.userAgent,
      platform: navigator.platform,
      language: navigator.language,
      cookieEnabled: navigator.cookieEnabled,
      onLine: navigator.onLine,
      screenWidth: screen.width,
      screenHeight: screen.height,
      windowWidth: window.innerWidth,
      windowHeight: window.innerHeight,
      timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
      url: window.location.href,
      referrer: document.referrer,
    };
  }

  private sanitizeUrl(url: string): string {
    if (!this.config.sanitizeUrls) return url;

    try {
      const urlObj = new URL(url);
      // Remove sensitive query parameters
      const sensitiveParams = ["token", "password", "key", "secret", "auth"];
      sensitiveParams.forEach((param) => {
        if (urlObj.searchParams.has(param)) {
          urlObj.searchParams.set(param, "[REDACTED]");
        }
      });
      return urlObj.toString();
    } catch {
      return url;
    }
  }

  private sanitizeMessage(message: string): string {
    if (message.length > this.config.maxLogEntrySize) {
      return (
        message.substring(0, this.config.maxLogEntrySize) + "... [TRUNCATED]"
      );
    }
    return message;
  }

  private createLogEntry(
    level: LogLevel,
    message: string,
    context?: string,
    details?: string,
    error?: Error
  ): FrontendLogEntry {
    const browserInfo = this.config.includeBrowserInfo
      ? this.getBrowserInfo()
      : undefined;

    let stackTrace: string | undefined;
    if (this.config.includeStackTrace && error?.stack) {
      stackTrace = error.stack;
    } else if (this.config.includeStackTrace && level === LogLevel.ERROR) {
      // Generate stack trace for error level logs even without an Error object
      stackTrace = new Error().stack;
    }

    return {
      level,
      message: this.sanitizeMessage(message),
      context,
      details,
      timestamp: new Date().toISOString(),
      browserInfo: browserInfo ? JSON.stringify(browserInfo) : undefined,
      userAgent: navigator.userAgent,
      stackTrace,
      url: this.sanitizeUrl(window.location.href),
      component: context,
      traceId: this.generateTraceId(),
    };
  }

  private shouldLog(level: LogLevel): boolean {
    if (!this.config.enabled) return false;

    const levelPriority = {
      [LogLevel.DEBUG]: 1,
      [LogLevel.INFO]: 2,
      [LogLevel.WARN]: 3,
      [LogLevel.ERROR]: 4,
      [LogLevel.CRITICAL]: 5,
    };

    return levelPriority[level] >= levelPriority[this.config.logLevel];
  }

  private queueLogEntry(entry: FrontendLogEntry): void {
    const queuedEntry: QueuedLogEntry = {
      ...entry,
      id: this.generateLogId(),
      retryCount: 0,
      queuedAt: new Date(),
    };

    this.logQueue.push(queuedEntry);

    // Process immediately if queue is getting full
    if (this.logQueue.length >= this.config.batchSize) {
      this.processQueue();
    }
  }

  private async processQueue(): Promise<void> {
    if (this.isProcessingQueue || this.logQueue.length === 0) {
      return;
    }

    this.isProcessingQueue = true;

    try {
      const batch = this.logQueue.splice(0, this.config.batchSize);
      await this.sendLogBatch(batch);
    } catch (error) {
      console.error("Failed to process log queue:", error);
    } finally {
      this.isProcessingQueue = false;
    }
  }

  private async sendLogBatch(batch: QueuedLogEntry[]): Promise<void> {
    const failedEntries: QueuedLogEntry[] = [];

    for (const entry of batch) {
      try {
        await this.sendLogEntry(entry);
      } catch (error) {
        console.error("Failed to send log entry:", error);

        if (entry.retryCount < this.config.maxRetries) {
          entry.retryCount++;
          failedEntries.push(entry);
        }
      }
    }

    // Re-queue failed entries for retry
    if (failedEntries.length > 0) {
      setTimeout(() => {
        this.logQueue.unshift(...failedEntries);
      }, this.config.retryDelay * Math.pow(2, failedEntries[0]?.retryCount || 1));
    }
  }

  private async sendLogEntry(entry: QueuedLogEntry): Promise<void> {
    const backendEntry = new BackendLogEntry({
      level: entry.level,
      message: entry.message,
      context: entry.context,
      details: entry.details,
      timestamp: entry.timestamp,
      browser_info: entry.browserInfo,
      user_agent: entry.userAgent,
      stack_trace: entry.stackTrace,
      url: entry.url,
      component: entry.component,
      trace_id: entry.traceId,
    });

    await LogFrontendMessage(backendEntry);
  }

  private flushQueue(): void {
    if (this.logQueue.length > 0) {
      this.processQueue();
    }
  }

  // Public logging methods
  debug(message: string, context?: string, details?: string): void {
    if (this.shouldLog(LogLevel.DEBUG)) {
      const entry = this.createLogEntry(
        LogLevel.DEBUG,
        message,
        context,
        details
      );
      this.queueLogEntry(entry);
    }
  }

  info(message: string, context?: string, details?: string): void {
    if (this.shouldLog(LogLevel.INFO)) {
      const entry = this.createLogEntry(
        LogLevel.INFO,
        message,
        context,
        details
      );
      this.queueLogEntry(entry);
    }
  }

  warn(message: string, context?: string, details?: string): void {
    if (this.shouldLog(LogLevel.WARN)) {
      const entry = this.createLogEntry(
        LogLevel.WARN,
        message,
        context,
        details
      );
      this.queueLogEntry(entry);
    }
  }

  error(
    message: string,
    context?: string,
    details?: string,
    error?: Error
  ): void {
    if (this.shouldLog(LogLevel.ERROR)) {
      const entry = this.createLogEntry(
        LogLevel.ERROR,
        message,
        context,
        details,
        error
      );
      this.queueLogEntry(entry);
    }
  }

  critical(
    message: string,
    context?: string,
    details?: string,
    error?: Error
  ): void {
    if (this.shouldLog(LogLevel.CRITICAL)) {
      const entry = this.createLogEntry(
        LogLevel.CRITICAL,
        message,
        context,
        details,
        error
      );
      this.queueLogEntry(entry);
      // Flush immediately for critical errors
      this.flushQueue();
    }
  }

  // Configuration methods
  setLogLevel(level: LogLevel): void {
    this.config.logLevel = level;
  }

  setEnabled(enabled: boolean): void {
    this.config.enabled = enabled;
  }

  updateConfig(config: Partial<LoggingConfig>): void {
    this.config = { ...this.config, ...config };
  }

  // Utility methods
  getQueueSize(): number {
    return this.logQueue.length;
  }

  clearQueue(): void {
    this.logQueue = [];
  }

  destroy(): void {
    if (this.flushTimer) {
      clearInterval(this.flushTimer);
    }
    this.flushQueue();
  }
}
