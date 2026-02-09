import { Injectable, NgZone, inject } from '@angular/core';
import { BehaviorSubject } from 'rxjs';
import { Events } from '@wailsio/runtime';
import {
  IsAuthEnabled,
  IsUnlocked,
  SetupPassword,
  Unlock,
  Lock,
  ChangePassword,
  RemovePassword,
  GetLockoutStatus,
} from '../../../wailsjs/desktop/backend/services/authservice';

export interface LockoutStatus {
  failed_attempts: number;
  locked_until: string;
  is_locked: boolean;
  retry_after_secs: number;
}

@Injectable({ providedIn: 'root' })
export class AuthService {
  private readonly ngZone = inject(NgZone);

  readonly isLocked$ = new BehaviorSubject<boolean>(true);
  readonly authEnabled$ = new BehaviorSubject<boolean>(false);
  readonly loading$ = new BehaviorSubject<boolean>(true);

  private eventCleanups: (() => void)[] = [];

  constructor() {
    this.listenEvents();
  }

  private listenEvents(): void {
    const cleanup1 = Events.On('auth:unlocked', () => {
      this.ngZone.run(() => {
        this.isLocked$.next(false);
        this.authEnabled$.next(true);
      });
    });
    const cleanup2 = Events.On('auth:locked', () => {
      this.ngZone.run(() => {
        this.isLocked$.next(true);
      });
    });
    if (cleanup1) this.eventCleanups.push(cleanup1);
    if (cleanup2) this.eventCleanups.push(cleanup2);
  }

  async checkStatus(): Promise<void> {
    try {
      this.loading$.next(true);
      const enabled = await IsAuthEnabled();
      this.authEnabled$.next(enabled);

      if (!enabled) {
        this.isLocked$.next(false);
      } else {
        const unlocked = await IsUnlocked();
        this.isLocked$.next(!unlocked);
      }
    } catch (err) {
      console.error('AuthService: Failed to check status:', err);
      this.isLocked$.next(false);
    } finally {
      this.loading$.next(false);
    }
  }

  async unlock(password: string): Promise<void> {
    await Unlock(password);
    // Update state directly after successful unlock (don't rely on events)
    this.isLocked$.next(false);
    this.authEnabled$.next(true);
  }

  async setupPassword(password: string): Promise<void> {
    await SetupPassword(password);
    // Update state directly after successful setup (don't rely on events)
    this.isLocked$.next(false);
    this.authEnabled$.next(true);
  }

  async lock(): Promise<void> {
    await Lock();
    // Update state directly after successful lock
    this.isLocked$.next(true);
  }

  async changePassword(oldPassword: string, newPassword: string): Promise<void> {
    await ChangePassword(oldPassword, newPassword);
  }

  async removePassword(password: string): Promise<void> {
    await RemovePassword(password);
    this.authEnabled$.next(false);
  }

  async getLockoutStatus(): Promise<LockoutStatus> {
    return await GetLockoutStatus() as unknown as LockoutStatus;
  }

  destroy(): void {
    this.eventCleanups.forEach(fn => fn());
    this.eventCleanups = [];
  }
}
