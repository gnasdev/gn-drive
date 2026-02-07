import { CommonModule } from '@angular/common';
import { ChangeDetectionStrategy, ChangeDetectorRef, Component, OnDestroy, OnInit, inject } from '@angular/core';
import { ConfirmDialog } from 'primeng/confirmdialog';
import { ToastModule } from 'primeng/toast';
import { Subscription } from 'rxjs';
import { AppService } from './app.service.js';
import { ErrorDisplayComponent } from './components/error-display/error-display.component.js';
import { TopbarComponent } from './components/topbar/topbar.component.js';
import { FlowsContainerComponent } from './components/flows/flows-container.component.js';
import { SettingsDialogComponent } from './components/dialogs/settings-dialog.component.js';
import { ConsoleLoggerService } from './services/console-logger.service.js';
import { LoggingService } from './services/logging.service.js';

@Component({
  selector: 'app-root',
  standalone: true,
  imports: [
    CommonModule,
    TopbarComponent,
    FlowsContainerComponent,
    SettingsDialogComponent,
    ErrorDisplayComponent,
    ToastModule,
    ConfirmDialog,
  ],
  templateUrl: './app.component.html',
  styleUrl: './app.component.css',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class AppComponent implements OnInit, OnDestroy {
  readonly appService = inject(AppService);
  private readonly cdr = inject(ChangeDetectorRef);
  private readonly loggingService = inject(LoggingService);
  private readonly consoleLoggerService = inject(ConsoleLoggerService);

  private subscriptions = new Subscription();
  private isInitialized = false;

  showSettingsDialog = false;

  ngOnInit() {
    if (this.isInitialized) {
      return;
    }

    this.isInitialized = true;

    // Defer logging and backend data loading after first paint
    requestAnimationFrame(() => {
      this.initializeLogging();
      this.appService.initialize();
    });
  }

  ngOnDestroy() {
    this.isInitialized = false;
    this.subscriptions.unsubscribe();
    this.consoleLoggerService.restoreConsole();
    this.loggingService.destroy();
  }

  openSettings(): void {
    this.showSettingsDialog = true;
    this.cdr.markForCheck();
  }

  private initializeLogging(): void {
    this.consoleLoggerService.initializeConsoleOverride();
    this.setupWindowErrorHandlers();
    this.loggingService.warn(
      'Application started',
      'app_startup',
      'Angular application initialized'
    );
  }

  private setupWindowErrorHandlers(): void {
    window.addEventListener('error', (event) => {
      this.loggingService.critical(
        `Unhandled Error: ${event.message}`,
        'window_error',
        `File: ${event.filename}, Line: ${event.lineno}, Column: ${event.colno}`,
        event.error
      );
    });

    window.addEventListener('unhandledrejection', (event) => {
      this.loggingService.critical(
        `Unhandled Promise Rejection: ${event.reason}`,
        'unhandled_promise_rejection',
        JSON.stringify(event.reason),
        event.reason instanceof Error ? event.reason : undefined
      );
    });

    window.addEventListener(
      'error',
      (event) => {
        if (event.target !== window) {
          const target = event.target as HTMLElement;
          this.loggingService.error(
            `Resource Load Error: ${
              (target as HTMLImageElement)?.src || (target as HTMLLinkElement)?.href
            }`,
            'resource_load_error',
            `Element: ${target?.tagName}, Type: ${(target as HTMLInputElement)?.type}`
          );
        }
      },
      true
    );
  }
}
