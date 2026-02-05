import { CommonModule } from "@angular/common";
import {
    ChangeDetectionStrategy,
    ChangeDetectorRef,
    Component,
    OnDestroy,
    OnInit,
} from "@angular/core";
import { ConfirmDialog } from "primeng/confirmdialog";
import { ToastModule } from "primeng/toast";
import { combineLatest, Subscription } from "rxjs";
import { AppService } from "./app.service.js";
import { BoardComponent } from "./board/board.component.js";
import { ErrorDisplayComponent } from "./components/error-display/error-display.component.js";
import { SidebarComponent } from "./components/sidebar/sidebar.component.js";
import { NavigationService } from "./navigation.service.js";
import { RemotesComponent } from "./remotes/remotes.component.js";
import { ConsoleLoggerService } from "./services/console-logger.service.js";
import { LoggingService } from "./services/logging.service.js";
import { SettingsComponent } from "./settings/settings.component.js";

@Component({
    selector: "app-root",
    imports: [
        CommonModule,
        SidebarComponent,
        BoardComponent,
        RemotesComponent,
        SettingsComponent,
        ErrorDisplayComponent,
        ToastModule,
        ConfirmDialog,
    ],
    templateUrl: "./app.component.html",
    styleUrl: "./app.component.css",
    changeDetection: ChangeDetectionStrategy.OnPush,
})
export class AppComponent implements OnInit, OnDestroy {
    private subscriptions = new Subscription();
    private isInitialized = false;

    constructor(
        public readonly appService: AppService,
        private readonly cdr: ChangeDetectorRef,
        public readonly navigationService: NavigationService,
        private readonly loggingService: LoggingService,
        private readonly consoleLoggerService: ConsoleLoggerService,
    ) {}

    ngOnInit() {
        if (this.isInitialized) {
            return;
        }

        this.isInitialized = true;

        this.subscriptions.add(
            combineLatest([
                this.appService.currentAction$,
                this.navigationService.currentState$,
            ]).subscribe(() => {
                this.cdr.detectChanges();
            }),
        );

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

    private initializeLogging(): void {
        this.consoleLoggerService.initializeConsoleOverride();
        this.setupWindowErrorHandlers();
        this.loggingService.warn(
            "Application started",
            "app_startup",
            "Angular application initialized",
        );
    }

    private setupWindowErrorHandlers(): void {
        window.addEventListener("error", (event) => {
            this.loggingService.critical(
                `Unhandled Error: ${event.message}`,
                "window_error",
                `File: ${event.filename}, Line: ${event.lineno}, Column: ${event.colno}`,
                event.error,
            );
        });

        window.addEventListener("unhandledrejection", (event) => {
            this.loggingService.critical(
                `Unhandled Promise Rejection: ${event.reason}`,
                "unhandled_promise_rejection",
                JSON.stringify(event.reason),
                event.reason instanceof Error ? event.reason : undefined,
            );
        });

        window.addEventListener(
            "error",
            (event) => {
                if (event.target !== window) {
                    const target = event.target as HTMLElement;
                    this.loggingService.error(
                        `Resource Load Error: ${
                            (target as HTMLImageElement)?.src ||
                            (target as HTMLLinkElement)?.href
                        }`,
                        "resource_load_error",
                        `Element: ${target?.tagName}, Type: ${
                            (target as HTMLInputElement)?.type
                        }`,
                    );
                }
            },
            true,
        );
    }
}
