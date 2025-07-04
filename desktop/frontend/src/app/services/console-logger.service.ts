import { Injectable } from "@angular/core";
import { LoggingService } from "./logging.service";
import { LogLevel } from "../models/logging.interface";

@Injectable({
  providedIn: "root",
})
export class ConsoleLoggerService {
  private originalConsole: {
    log: typeof console.log;
    warn: typeof console.warn;
    error: typeof console.error;
    info: typeof console.info;
    debug: typeof console.debug;
  };

  private isInitialized = false;

  constructor(private loggingService: LoggingService) {
    // Store original console methods
    this.originalConsole = {
      log: console.log.bind(console),
      warn: console.warn.bind(console),
      error: console.error.bind(console),
      info: console.info.bind(console),
      debug: console.debug.bind(console),
    };
  }

  initializeConsoleOverride(): void {
    if (this.isInitialized) {
      return;
    }

    this.isInitialized = true;

    // Override console.log (but don't send to backend - only keep local)
    console.log = (...args: any[]) => {
      this.originalConsole.log(...args);
      // Don't send console.log to backend
    };

    // Override console.info (but don't send to backend - only keep local)
    console.info = (...args: any[]) => {
      this.originalConsole.info(...args);
      // Don't send console.info to backend
    };

    // Override console.warn
    console.warn = (...args: any[]) => {
      this.originalConsole.warn(...args);
      this.logToBackend(LogLevel.WARN, args, "console.warn");
    };

    // Override console.error
    console.error = (...args: any[]) => {
      this.originalConsole.error(...args);
      this.logToBackend(LogLevel.ERROR, args, "console.error");
    };

    // Override console.debug (but don't send to backend - only keep local)
    console.debug = (...args: any[]) => {
      this.originalConsole.debug(...args);
      // Don't send console.debug to backend
    };

    console.info("Console logging override initialized");
  }

  restoreConsole(): void {
    if (!this.isInitialized) {
      return;
    }

    console.log = this.originalConsole.log;
    console.warn = this.originalConsole.warn;
    console.error = this.originalConsole.error;
    console.info = this.originalConsole.info;
    console.debug = this.originalConsole.debug;

    this.isInitialized = false;
    console.info("Console logging override restored");
  }

  private logToBackend(level: LogLevel, args: any[], context: string): void {
    try {
      const message = this.formatConsoleArgs(args);
      const details = this.getConsoleDetails(args);

      // Don't log our own logging messages to avoid infinite loops
      if (
        message.includes("Console logging") ||
        message.includes("LoggingService")
      ) {
        return;
      }

      switch (level) {
        case LogLevel.DEBUG:
          this.loggingService.debug(message, context, details);
          break;
        case LogLevel.INFO:
          this.loggingService.info(message, context, details);
          break;
        case LogLevel.WARN:
          this.loggingService.warn(message, context, details);
          break;
        case LogLevel.ERROR:
          this.loggingService.error(message, context, details);
          break;
      }
    } catch (error) {
      // Fallback to original console to avoid infinite loops
      this.originalConsole.error(
        "Failed to log console message to backend:",
        error
      );
    }
  }

  private formatConsoleArgs(args: any[]): string {
    return args
      .map((arg) => {
        if (typeof arg === "string") {
          return arg;
        }
        if (arg instanceof Error) {
          return `${arg.name}: ${arg.message}`;
        }
        try {
          return JSON.stringify(arg);
        } catch {
          return String(arg);
        }
      })
      .join(" ");
  }

  private getConsoleDetails(args: any[]): string {
    try {
      return JSON.stringify(
        args.map((arg) => {
          if (arg instanceof Error) {
            return {
              name: arg.name,
              message: arg.message,
              stack: arg.stack,
            };
          }
          return arg;
        }),
        null,
        2
      );
    } catch {
      return "Failed to serialize console arguments";
    }
  }

  // Method to manually log without console override
  logDirect(
    level: LogLevel,
    message: string,
    context?: string,
    details?: string
  ): void {
    switch (level) {
      case LogLevel.DEBUG:
        this.originalConsole.debug(message);
        break;
      case LogLevel.INFO:
        this.originalConsole.info(message);
        break;
      case LogLevel.WARN:
        this.originalConsole.warn(message);
        break;
      case LogLevel.ERROR:
        this.originalConsole.error(message);
        break;
    }

    this.logToBackend(level, [message], context || "direct_log");
  }
}
