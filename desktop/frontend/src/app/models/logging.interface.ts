export enum LogLevel {
  DEBUG = 'DEBUG',
  INFO = 'INFO',
  WARN = 'WARN',
  ERROR = 'ERROR',
  CRITICAL = 'CRITICAL'
}

export interface FrontendLogEntry {
  level: LogLevel;
  message: string;
  context?: string;
  details?: string;
  timestamp: string; // ISO string
  browserInfo?: string;
  userAgent?: string;
  stackTrace?: string;
  url?: string;
  component?: string;
  traceId?: string;
}

export interface BrowserInfo {
  userAgent: string;
  platform: string;
  language: string;
  cookieEnabled: boolean;
  onLine: boolean;
  screenWidth: number;
  screenHeight: number;
  windowWidth: number;
  windowHeight: number;
  timezone: string;
  url: string;
  referrer: string;
}

export interface LoggingConfig {
  enabled: boolean;
  logLevel: LogLevel;
  maxRetries: number;
  retryDelay: number;
  batchSize: number;
  flushInterval: number;
  includeStackTrace: boolean;
  includeBrowserInfo: boolean;
  sanitizeUrls: boolean;
  maxLogEntrySize: number;
}

export interface QueuedLogEntry extends FrontendLogEntry {
  id: string;
  retryCount: number;
  queuedAt: Date;
}
