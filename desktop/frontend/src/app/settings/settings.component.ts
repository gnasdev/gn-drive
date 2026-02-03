import { CommonModule } from "@angular/common";
import { ChangeDetectionStrategy, ChangeDetectorRef, Component } from "@angular/core";
import { FormsModule } from "@angular/forms";
import {
  LucideAngularModule,
  Settings,
  Bell,
  Lock,
  Bug,
  FolderOpen,
  Info,
} from "lucide-angular";

@Component({
  selector: "app-settings",
  standalone: true,
  imports: [CommonModule, FormsModule, LucideAngularModule],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="flex flex-col h-full">
      <!-- Header -->
      <div class="flex items-center gap-3 p-4 border-b border-gray-700">
        <lucide-icon
          [img]="SettingsIcon"
          class="w-5 h-5 text-primary-400"
        ></lucide-icon>
        <h1 class="text-lg font-semibold text-gray-100">Settings</h1>
      </div>

      <!-- Settings sections -->
      <div class="flex-1 overflow-auto p-4 space-y-4">
        <!-- Notifications -->
        <div class="panel">
          <div class="flex items-center gap-2 mb-3">
            <lucide-icon
              [img]="BellIcon"
              class="w-4 h-4 text-gray-400"
            ></lucide-icon>
            <h2 class="text-sm font-semibold text-gray-200">Notifications</h2>
          </div>
          <div class="flex items-center justify-between">
            <div>
              <p class="text-sm text-gray-300">Desktop Notifications</p>
              <p class="text-xs text-gray-500">
                Show notifications when operations complete or fail
              </p>
            </div>
            <label class="relative inline-flex items-center cursor-pointer">
              <input
                type="checkbox"
                [(ngModel)]="notificationsEnabled"
                class="sr-only peer"
                (change)="saveNotificationSetting()"
              />
              <div
                class="w-9 h-5 bg-gray-600 peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:rounded-full after:h-4 after:w-4 after:transition-all peer-checked:bg-primary-600"
              ></div>
            </label>
          </div>
        </div>

        <!-- Security -->
        <div class="panel">
          <div class="flex items-center gap-2 mb-3">
            <lucide-icon
              [img]="LockIcon"
              class="w-4 h-4 text-gray-400"
            ></lucide-icon>
            <h2 class="text-sm font-semibold text-gray-200">Security</h2>
          </div>
          <div class="flex items-center justify-between">
            <div>
              <p class="text-sm text-gray-300">Config Encryption</p>
              <p class="text-xs text-gray-500">
                Encrypt rclone configuration file with a password
              </p>
            </div>
            <label class="relative inline-flex items-center cursor-pointer">
              <input
                type="checkbox"
                [(ngModel)]="configEncrypted"
                class="sr-only peer"
                (change)="toggleConfigEncryption()"
              />
              <div
                class="w-9 h-5 bg-gray-600 peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:rounded-full after:h-4 after:w-4 after:transition-all peer-checked:bg-primary-600"
              ></div>
            </label>
          </div>
        </div>

        <!-- Debug -->
        <div class="panel">
          <div class="flex items-center gap-2 mb-3">
            <lucide-icon
              [img]="BugIcon"
              class="w-4 h-4 text-gray-400"
            ></lucide-icon>
            <h2 class="text-sm font-semibold text-gray-200">Debug</h2>
          </div>
          <div class="flex items-center justify-between">
            <div>
              <p class="text-sm text-gray-300">Debug Mode</p>
              <p class="text-xs text-gray-500">
                Enable verbose logging for troubleshooting
              </p>
            </div>
            <label class="relative inline-flex items-center cursor-pointer">
              <input
                type="checkbox"
                [(ngModel)]="debugMode"
                class="sr-only peer"
                (change)="saveDebugSetting()"
              />
              <div
                class="w-9 h-5 bg-gray-600 peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:rounded-full after:h-4 after:w-4 after:transition-all peer-checked:bg-primary-600"
              ></div>
            </label>
          </div>
        </div>

        <!-- Paths -->
        <div class="panel">
          <div class="flex items-center gap-2 mb-3">
            <lucide-icon
              [img]="FolderOpenIcon"
              class="w-4 h-4 text-gray-400"
            ></lucide-icon>
            <h2 class="text-sm font-semibold text-gray-200">Configuration Paths</h2>
          </div>
          <div class="space-y-2">
            <div class="flex justify-between text-sm">
              <span class="text-gray-400">Profiles</span>
              <span class="text-gray-500 font-mono text-xs"
                >~/.config/ns-drive/profiles.json</span
              >
            </div>
            <div class="flex justify-between text-sm">
              <span class="text-gray-400">Rclone Config</span>
              <span class="text-gray-500 font-mono text-xs"
                >~/.config/ns-drive/rclone.conf</span
              >
            </div>
            <div class="flex justify-between text-sm">
              <span class="text-gray-400">History</span>
              <span class="text-gray-500 font-mono text-xs"
                >~/.config/ns-drive/history.json</span
              >
            </div>
            <div class="flex justify-between text-sm">
              <span class="text-gray-400">Schedules</span>
              <span class="text-gray-500 font-mono text-xs"
                >~/.config/ns-drive/schedules.json</span
              >
            </div>
          </div>
        </div>

        <!-- About -->
        <div class="panel">
          <div class="flex items-center gap-2 mb-3">
            <lucide-icon
              [img]="InfoIcon"
              class="w-4 h-4 text-gray-400"
            ></lucide-icon>
            <h2 class="text-sm font-semibold text-gray-200">About</h2>
          </div>
          <div class="space-y-2">
            <div class="flex justify-between text-sm">
              <span class="text-gray-400">NS-Drive Version</span>
              <span class="text-gray-300">1.0.0</span>
            </div>
            <div class="flex justify-between text-sm">
              <span class="text-gray-400">Powered by</span>
              <span class="text-gray-300">rclone + Wails v3</span>
            </div>
          </div>
        </div>
      </div>
    </div>
  `,
})
export class SettingsComponent {
  readonly SettingsIcon = Settings;
  readonly BellIcon = Bell;
  readonly LockIcon = Lock;
  readonly BugIcon = Bug;
  readonly FolderOpenIcon = FolderOpen;
  readonly InfoIcon = Info;

  notificationsEnabled = true;
  configEncrypted = false;
  debugMode = false;

  constructor(private readonly cdr: ChangeDetectorRef) {}

  saveNotificationSetting() {
    // TODO: Call NotificationService.SetEnabled via Wails bindings
  }

  toggleConfigEncryption() {
    // TODO: Show password dialog, call CryptService via Wails bindings
  }

  saveDebugSetting() {
    // TODO: Persist debug mode setting
  }
}
