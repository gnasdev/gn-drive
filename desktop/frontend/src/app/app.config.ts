/** GN Drive note: Supports the Angular frontend for app. */
import { ApplicationConfig, ErrorHandler, provideZonelessChangeDetection } from "@angular/core";
import { provideHttpClient, HTTP_INTERCEPTORS } from "@angular/common/http";
import { provideAnimationsAsync } from "@angular/platform-browser/animations/async";
import { providePrimeNG } from "primeng/config";
import { MessageService, ConfirmationService } from "primeng/api";
import { GlobalErrorHandler } from "./services/global-error-handler.service";
import { ErrorInterceptor } from "./interceptors/error.interceptor";
import NsDrivePreset from "./primeng-preset";
import { provideWailsLogServiceClient } from "./services/wails-log-service-client";

export const appConfig: ApplicationConfig = {
  providers: [
    provideZonelessChangeDetection(),
    provideHttpClient(),
    provideAnimationsAsync(),
    providePrimeNG({
      theme: {
        preset: NsDrivePreset,
        options: {
          darkModeSelector: ".dark-theme",
        },
      },
      ripple: false,
    }),
    MessageService,
    ConfirmationService,
    provideWailsLogServiceClient(),
    {
      provide: HTTP_INTERCEPTORS,
      useClass: ErrorInterceptor,
      multi: true,
    },
    {
      provide: ErrorHandler,
      useClass: GlobalErrorHandler,
    },
  ],
};
