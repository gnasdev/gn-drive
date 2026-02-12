import { ApplicationConfig, ErrorHandler, provideZonelessChangeDetection } from "@angular/core";
import { provideHttpClient, HTTP_INTERCEPTORS } from "@angular/common/http";
import { provideAnimationsAsync } from "@angular/platform-browser/animations/async";
import { providePrimeNG } from "primeng/config";
import { MessageService, ConfirmationService } from "primeng/api";
import { GlobalErrorHandler } from "./services/global-error-handler.service";
import { ErrorInterceptor } from "./interceptors/error.interceptor";
import NsDrivePreset from "./primeng-preset";

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
