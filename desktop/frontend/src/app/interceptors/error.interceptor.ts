import { Injectable } from '@angular/core';
import { HttpInterceptor, HttpRequest, HttpHandler, HttpEvent, HttpErrorResponse } from '@angular/common/http';
import { Observable, throwError } from 'rxjs';
import { catchError, retry } from 'rxjs/operators';
import { ErrorService } from '../services/error.service';

@Injectable()
export class ErrorInterceptor implements HttpInterceptor {
  constructor(private errorService: ErrorService) {}

  intercept(req: HttpRequest<any>, next: HttpHandler): Observable<HttpEvent<any>> {
    return next.handle(req).pipe(
      // Retry failed requests up to 2 times for certain error types
      retry({
        count: 2,
        delay: (error: HttpErrorResponse, retryCount: number) => {
          // Only retry for network errors and 5xx server errors
          if (this.shouldRetry(error)) {
            console.log(`Retrying request (attempt ${retryCount + 1}):`, req.url);
            // Exponential backoff: 1s, 2s, 4s...
            return new Promise(resolve => setTimeout(resolve, Math.pow(2, retryCount) * 1000));
          }
          // Don't retry, let the error through
          throw error;
        }
      }),
      catchError((error: HttpErrorResponse) => {
        this.handleHttpError(error, req);
        return throwError(() => error);
      })
    );
  }

  private shouldRetry(error: HttpErrorResponse): boolean {
    // Retry for network errors
    if (error.status === 0) {
      return true;
    }

    // Retry for server errors (5xx)
    if (error.status >= 500 && error.status < 600) {
      return true;
    }

    // Retry for specific timeout errors
    if (error.status === 408 || error.status === 504) {
      return true;
    }

    return false;
  }

  private handleHttpError(error: HttpErrorResponse, req: HttpRequest<any>): void {
    const context = `${req.method} ${req.url}`;

    switch (error.status) {
      case 0:
        // Network error
        this.errorService.handleNetworkError(error);
        break;

      case 400:
        // Bad Request - usually validation errors
        this.errorService.handleApiError(error, context);
        break;

      case 401:
        // Unauthorized
        this.errorService.handleApiError({
          error: {
            code: 'AUTHENTICATION_ERROR',
            message: 'Authentication required. Please log in.',
            timestamp: new Date().toISOString(),
            trace_id: this.generateTraceId()
          }
        }, context);
        break;

      case 403:
        // Forbidden
        this.errorService.handleApiError({
          error: {
            code: 'AUTHORIZATION_ERROR',
            message: 'You do not have permission to perform this action.',
            timestamp: new Date().toISOString(),
            trace_id: this.generateTraceId()
          }
        }, context);
        break;

      case 404:
        // Not Found
        this.errorService.handleApiError({
          error: {
            code: 'NOT_FOUND_ERROR',
            message: 'The requested resource was not found.',
            timestamp: new Date().toISOString(),
            trace_id: this.generateTraceId()
          }
        }, context);
        break;

      case 408:
      case 504:
        // Timeout errors
        this.errorService.handleTimeoutError();
        break;

      case 409:
        // Conflict
        this.errorService.handleApiError({
          error: {
            code: 'CONFLICT_ERROR',
            message: 'The request conflicts with the current state of the resource.',
            timestamp: new Date().toISOString(),
            trace_id: this.generateTraceId()
          }
        }, context);
        break;

      case 422:
        // Unprocessable Entity - validation errors
        this.errorService.handleApiError(error, context);
        break;

      case 429:
        // Too Many Requests
        this.errorService.handleApiError({
          error: {
            code: 'RATE_LIMIT_ERROR',
            message: 'Too many requests. Please wait before trying again.',
            timestamp: new Date().toISOString(),
            trace_id: this.generateTraceId()
          }
        }, context);
        break;

      case 500:
      case 502:
      case 503:
        // Server errors
        this.errorService.handleApiError({
          error: {
            code: 'INTERNAL_ERROR',
            message: 'A server error occurred. Please try again later.',
            details: error.message,
            timestamp: new Date().toISOString(),
            trace_id: this.generateTraceId()
          }
        }, context);
        break;

      default:
        // Generic error handling
        this.errorService.handleApiError(error, context);
        break;
    }
  }

  private generateTraceId(): string {
    return `client_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
  }
}
