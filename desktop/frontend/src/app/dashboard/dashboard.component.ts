import { CommonModule } from "@angular/common";
import { ChangeDetectionStrategy, Component } from "@angular/core";
import { Card } from "primeng/card";
import { ButtonModule } from "primeng/button";
import { NavigationService } from "../navigation.service.js";

@Component({
  selector: "app-dashboard",
  standalone: true,
  imports: [CommonModule, Card, ButtonModule],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="p-6 space-y-6">
      <h1 class="text-2xl font-bold text-gray-100">Dashboard</h1>

      <div class="grid grid-cols-1 lg:grid-cols-2 gap-5">
        <!-- Quick Actions -->
        <p-card>
          <h2 class="text-sm font-semibold text-gray-400 uppercase tracking-wider mb-4">
            Quick Actions
          </h2>
          <div class="space-y-3">
            <p-button
              styleClass="w-full"
              severity="secondary"
              [outlined]="true"
              (onClick)="navigationService.navigateToOperations()"
            >
              <i class="pi pi-play text-primary-400"></i>
              <span>Start New Operation</span>
            </p-button>
            <p-button
              styleClass="w-full"
              severity="secondary"
              [outlined]="true"
              (onClick)="navigationService.navigateToSchedules()"
            >
              <i class="pi pi-clock text-primary-400"></i>
              <span>Manage Schedules</span>
            </p-button>
          </div>
        </p-card>

        <!-- Active Operations -->
        <p-card>
          <h2 class="text-sm font-semibold text-gray-400 uppercase tracking-wider mb-4">
            Active Operations
          </h2>
          <div class="flex items-center justify-center h-24 text-gray-500">
            <div class="text-center">
              <i class="pi pi-chart-line text-2xl mx-auto mb-2 opacity-40"></i>
              <p class="text-sm">No active operations</p>
            </div>
          </div>
        </p-card>

        <!-- Remote Storage -->
        <p-card>
          <h2 class="text-sm font-semibold text-gray-400 uppercase tracking-wider mb-4">
            Remote Storage
          </h2>
          <div class="flex items-center justify-center h-24 text-gray-500">
            <div class="text-center">
              <i class="pi pi-server text-2xl mx-auto mb-2 opacity-40"></i>
              <p class="text-sm">Configure remotes to see quota info</p>
            </div>
          </div>
        </p-card>

        <!-- Recent Activity -->
        <p-card>
          <h2 class="text-sm font-semibold text-gray-400 uppercase tracking-wider mb-4">
            Recent Activity
          </h2>
          <div class="flex items-center justify-center h-24 text-gray-500">
            <div class="text-center">
              <i class="pi pi-clock text-2xl mx-auto mb-2 opacity-40"></i>
              <p class="text-sm">No recent activity</p>
            </div>
          </div>
        </p-card>
      </div>
    </div>
  `,
})
export class DashboardComponent {
  constructor(public readonly navigationService: NavigationService) {}
}
