import { CommonModule } from "@angular/common";
import { ChangeDetectionStrategy, ChangeDetectorRef, Component, OnInit } from "@angular/core";
import { FormsModule } from "@angular/forms";
import { Card } from "primeng/card";
import { Toolbar } from "primeng/toolbar";
import { ToggleSwitch } from "primeng/toggleswitch";
import {
  GetSettings,
  SetEnabled,
  SetDebugMode,
} from "../../../wailsjs/desktop/backend/services/notificationservice";

@Component({
  selector: "app-settings",
  standalone: true,
  imports: [CommonModule, FormsModule, Card, Toolbar, ToggleSwitch],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="flex flex-col h-full">
      <!-- Header -->
      <p-toolbar>
        <ng-template #start>
          <div class="flex items-center gap-3">
            <i class="pi pi-cog text-primary-400"></i>
            <h1 class="text-lg font-semibold text-gray-100">Settings</h1>
          </div>
        </ng-template>
      </p-toolbar>

      <!-- Settings sections -->
      <div class="flex-1 overflow-auto p-4 space-y-4">
        <!-- Notifications -->
        <p-card>
          <div class="flex items-center gap-2 mb-3">
            <i class="pi pi-bell text-gray-400"></i>
            <h2 class="text-sm font-semibold text-gray-200">Notifications</h2>
          </div>
          <div class="flex items-center justify-between">
            <div>
              <p class="text-sm text-gray-300">Desktop Notifications</p>
              <p class="text-xs text-gray-500">
                Show notifications when operations complete or fail
              </p>
            </div>
            <p-toggleswitch
              [(ngModel)]="notificationsEnabled"
              (onChange)="saveNotificationSetting()"
            ></p-toggleswitch>
          </div>
        </p-card>

        <!-- Security -->
        <p-card>
          <div class="flex items-center gap-2 mb-3">
            <i class="pi pi-lock text-gray-400"></i>
            <h2 class="text-sm font-semibold text-gray-200">Security</h2>
          </div>
          <div class="flex items-center justify-between">
            <div>
              <p class="text-sm text-gray-300">Config Encryption</p>
              <p class="text-xs text-gray-500">
                Encrypt rclone configuration file with a password
              </p>
            </div>
            <p-toggleswitch
              [(ngModel)]="configEncrypted"
              (onChange)="toggleConfigEncryption()"
            ></p-toggleswitch>
          </div>
        </p-card>

        <!-- Debug -->
        <p-card>
          <div class="flex items-center gap-2 mb-3">
            <i class="pi pi-wrench text-gray-400"></i>
            <h2 class="text-sm font-semibold text-gray-200">Debug</h2>
          </div>
          <div class="flex items-center justify-between">
            <div>
              <p class="text-sm text-gray-300">Debug Mode</p>
              <p class="text-xs text-gray-500">
                Enable verbose logging for troubleshooting
              </p>
            </div>
            <p-toggleswitch
              [(ngModel)]="debugMode"
              (onChange)="saveDebugSetting()"
            ></p-toggleswitch>
          </div>
        </p-card>

        <!-- Paths -->
        <p-card>
          <div class="flex items-center gap-2 mb-3">
            <i class="pi pi-folder-open text-gray-400"></i>
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
        </p-card>

        <!-- About -->
        <p-card>
          <div class="flex items-center gap-2 mb-3">
            <i class="pi pi-info-circle text-gray-400"></i>
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
        </p-card>
      </div>
    </div>
  `,
})
export class SettingsComponent implements OnInit {
  notificationsEnabled = true;
  configEncrypted = false;
  debugMode = false;

  constructor(private readonly cdr: ChangeDetectorRef) {}

  async ngOnInit() {
    try {
      const settings = await GetSettings();
      this.notificationsEnabled = settings.notifications_enabled;
      this.debugMode = settings.debug_mode;
      this.cdr.markForCheck();
    } catch (e) {
      console.error("Failed to load settings:", e);
    }
  }

  async saveNotificationSetting() {
    try {
      await SetEnabled(this.notificationsEnabled);
    } catch (e) {
      console.error("Failed to save notification setting:", e);
    }
  }

  toggleConfigEncryption() {
    // Config encryption requires password dialog — not yet implemented
  }

  async saveDebugSetting() {
    try {
      await SetDebugMode(this.debugMode);
    } catch (e) {
      console.error("Failed to save debug setting:", e);
    }
  }
}
