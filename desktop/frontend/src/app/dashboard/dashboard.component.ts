import { CommonModule } from "@angular/common";
import { ChangeDetectionStrategy, Component } from "@angular/core";
import {
  LucideAngularModule,
  Play,
  Clock,
  HardDrive,
  Activity,
} from "lucide-angular";
import { NavigationService } from "../navigation.service.js";

@Component({
  selector: "app-dashboard",
  standalone: true,
  imports: [CommonModule, LucideAngularModule],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="p-6 space-y-6">
      <h1 class="text-2xl font-bold text-gray-100">Dashboard</h1>

      <div class="grid grid-cols-1 lg:grid-cols-2 gap-5">
        <!-- Quick Actions -->
        <div class="panel">
          <h2 class="text-sm font-semibold text-gray-400 uppercase tracking-wider mb-4">
            Quick Actions
          </h2>
          <div class="space-y-3">
            <button
              (click)="navigationService.navigateToOperations()"
              class="w-full flex items-center gap-3 px-4 py-3 rounded-lg bg-gray-700/50 hover:bg-gray-700 text-gray-200 transition-colors"
            >
              <lucide-icon [img]="PlayIcon" class="w-5 h-5 text-primary-400"></lucide-icon>
              <span>Start New Operation</span>
            </button>
            <button
              (click)="navigationService.navigateToSchedules()"
              class="w-full flex items-center gap-3 px-4 py-3 rounded-lg bg-gray-700/50 hover:bg-gray-700 text-gray-200 transition-colors"
            >
              <lucide-icon [img]="ClockIcon" class="w-5 h-5 text-primary-400"></lucide-icon>
              <span>Manage Schedules</span>
            </button>
          </div>
        </div>

        <!-- Active Operations -->
        <div class="panel">
          <h2 class="text-sm font-semibold text-gray-400 uppercase tracking-wider mb-4">
            Active Operations
          </h2>
          <div class="flex items-center justify-center h-24 text-gray-500">
            <div class="text-center">
              <lucide-icon [img]="ActivityIcon" class="w-8 h-8 mx-auto mb-2 opacity-40"></lucide-icon>
              <p class="text-sm">No active operations</p>
            </div>
          </div>
        </div>

        <!-- Remote Storage -->
        <div class="panel">
          <h2 class="text-sm font-semibold text-gray-400 uppercase tracking-wider mb-4">
            Remote Storage
          </h2>
          <div class="flex items-center justify-center h-24 text-gray-500">
            <div class="text-center">
              <lucide-icon [img]="HardDriveIcon" class="w-8 h-8 mx-auto mb-2 opacity-40"></lucide-icon>
              <p class="text-sm">Configure remotes to see quota info</p>
            </div>
          </div>
        </div>

        <!-- Recent Activity -->
        <div class="panel">
          <h2 class="text-sm font-semibold text-gray-400 uppercase tracking-wider mb-4">
            Recent Activity
          </h2>
          <div class="flex items-center justify-center h-24 text-gray-500">
            <div class="text-center">
              <lucide-icon [img]="ClockIcon" class="w-8 h-8 mx-auto mb-2 opacity-40"></lucide-icon>
              <p class="text-sm">No recent activity</p>
            </div>
          </div>
        </div>
      </div>
    </div>
  `,
})
export class DashboardComponent {
  readonly PlayIcon = Play;
  readonly ClockIcon = Clock;
  readonly HardDriveIcon = HardDrive;
  readonly ActivityIcon = Activity;

  constructor(public readonly navigationService: NavigationService) {}
}
