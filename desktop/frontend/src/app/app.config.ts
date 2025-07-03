import { ApplicationConfig, ErrorHandler } from "@angular/core";
import { provideHttpClient, HTTP_INTERCEPTORS } from "@angular/common/http";
import { GlobalErrorHandler } from "./services/global-error-handler.service";
import { ErrorInterceptor } from "./interceptors/error.interceptor";

export const appConfig: ApplicationConfig = {
  providers: [
    provideHttpClient(),
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
