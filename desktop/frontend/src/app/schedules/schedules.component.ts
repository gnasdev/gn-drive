import { CommonModule } from "@angular/common";
import { ChangeDetectionStrategy, ChangeDetectorRef, Component } from "@angular/core";
import { FormsModule } from "@angular/forms";
import {
  LucideAngularModule,
  Calendar,
  Plus,
  Trash2,
  Play,
  ToggleLeft,
  ToggleRight,
} from "lucide-angular";

interface ScheduleEntry {
  id: string;
  profile_name: string;
  action: string;
  cron_expr: string;
  enabled: boolean;
  last_run?: string;
  next_run?: string;
  last_result?: string;
}

@Component({
  selector: "app-schedules",
  standalone: true,
  imports: [CommonModule, FormsModule, LucideAngularModule],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="flex flex-col h-full">
      <!-- Header -->
      <div
        class="flex items-center justify-between p-4 border-b border-gray-700"
      >
        <div class="flex items-center gap-3">
          <lucide-icon
            [img]="CalendarIcon"
            class="w-5 h-5 text-primary-400"
          ></lucide-icon>
          <h1 class="text-lg font-semibold text-gray-100">Schedules</h1>
          <span class="text-sm text-gray-500"
            >{{ schedules.length }} schedule{{
              schedules.length !== 1 ? "s" : ""
            }}</span
          >
        </div>
        <button
          (click)="showAddModal = true"
          class="btn-primary flex items-center gap-1.5 text-sm"
        >
          <lucide-icon [img]="PlusIcon" class="w-4 h-4"></lucide-icon>
          Add Schedule
        </button>
      </div>

      <!-- Schedule list -->
      <div class="flex-1 overflow-auto p-4">
        @if (schedules.length === 0) {
        <div
          class="flex flex-col items-center justify-center h-48 text-gray-500"
        >
          <lucide-icon
            [img]="CalendarIcon"
            class="w-12 h-12 mb-3 opacity-30"
          ></lucide-icon>
          <p class="text-sm mb-3">No schedules configured</p>
          <button
            (click)="showAddModal = true"
            class="btn-primary text-sm"
          >
            Create First Schedule
          </button>
        </div>
        } @else { @for (schedule of schedules; track schedule.id) {
        <div
          class="panel mb-3 flex items-center justify-between"
        >
          <div class="flex-1 min-w-0">
            <div class="flex items-center gap-2 mb-1">
              <span class="text-gray-200 font-medium">{{
                schedule.profile_name
              }}</span>
              <span
                class="text-xs px-2 py-0.5 rounded bg-gray-700 text-gray-400"
                >{{ schedule.action }}</span
              >
            </div>
            <div class="text-xs text-gray-500">
              Cron: {{ schedule.cron_expr }}
              @if (schedule.next_run) { · Next:
              {{ schedule.next_run }} } @if (schedule.last_result) {
              <span
                [class]="
                  schedule.last_result === 'success'
                    ? 'text-green-400'
                    : 'text-red-400'
                "
              >
                · Last: {{ schedule.last_result }}
              </span>
              }
            </div>
          </div>
          <div class="flex items-center gap-2">
            <button
              (click)="toggleSchedule(schedule)"
              class="p-1 rounded hover:bg-gray-700 transition-colors"
              [title]="schedule.enabled ? 'Disable' : 'Enable'"
            >
              <lucide-icon
                [img]="schedule.enabled ? ToggleRightIcon : ToggleLeftIcon"
                class="w-5 h-5"
                [class]="
                  schedule.enabled ? 'text-green-400' : 'text-gray-500'
                "
              ></lucide-icon>
            </button>
            <button
              class="p-1 rounded hover:bg-gray-700 transition-colors"
              title="Run Now"
            >
              <lucide-icon
                [img]="PlayIcon"
                class="w-4 h-4 text-gray-400"
              ></lucide-icon>
            </button>
            <button
              (click)="deleteSchedule(schedule)"
              class="p-1 rounded hover:bg-gray-700 transition-colors"
              title="Delete"
            >
              <lucide-icon
                [img]="Trash2Icon"
                class="w-4 h-4 text-gray-400 hover:text-red-400"
              ></lucide-icon>
            </button>
          </div>
        </div>
        } }
      </div>

      <!-- Add Schedule Modal -->
      @if (showAddModal) {
      <div
        class="fixed inset-0 bg-black/60 flex items-center justify-center z-50"
        (click)="showAddModal = false"
      >
        <div
          class="bg-gray-800 rounded-xl border border-gray-700 p-6 w-96 max-w-[90vw]"
          (click)="$event.stopPropagation()"
        >
          <h2 class="text-lg font-semibold text-gray-100 mb-4">
            Add Schedule
          </h2>

          <div class="space-y-3">
            <div>
              <label class="text-sm text-gray-400">Profile Name</label>
              <input
                type="text"
                [(ngModel)]="newSchedule.profile_name"
                class="input-field mt-1"
                placeholder="my-backup"
              />
            </div>
            <div>
              <label class="text-sm text-gray-400">Action</label>
              <select [(ngModel)]="newSchedule.action" class="select-field mt-1">
                <option value="pull">Pull</option>
                <option value="push">Push</option>
                <option value="bi">Bi-Sync</option>
                <option value="bi-resync">Bi-Resync</option>
              </select>
            </div>
            <div>
              <label class="text-sm text-gray-400">Cron Expression</label>
              <input
                type="text"
                [(ngModel)]="newSchedule.cron_expr"
                class="input-field mt-1"
                placeholder="0 */6 * * *"
              />
              <p class="text-xs text-gray-500 mt-1">
                Standard 5-field cron: min hour dom month dow
              </p>
            </div>
          </div>

          <div class="flex justify-end gap-2 mt-5">
            <button
              (click)="showAddModal = false"
              class="btn-secondary text-sm"
            >
              Cancel
            </button>
            <button (click)="addSchedule()" class="btn-primary text-sm">
              Add
            </button>
          </div>
        </div>
      </div>
      }
    </div>
  `,
})
export class SchedulesComponent {
  readonly CalendarIcon = Calendar;
  readonly PlusIcon = Plus;
  readonly Trash2Icon = Trash2;
  readonly PlayIcon = Play;
  readonly ToggleLeftIcon = ToggleLeft;
  readonly ToggleRightIcon = ToggleRight;

  schedules: ScheduleEntry[] = [];
  showAddModal = false;
  newSchedule = { profile_name: "", action: "push", cron_expr: "" };

  constructor(private readonly cdr: ChangeDetectorRef) {}

  addSchedule() {
    // TODO: Call SchedulerService via Wails bindings
    this.showAddModal = false;
    this.newSchedule = { profile_name: "", action: "push", cron_expr: "" };
  }

  toggleSchedule(schedule: ScheduleEntry) {
    schedule.enabled = !schedule.enabled;
    this.cdr.detectChanges();
    // TODO: Call SchedulerService.Enable/Disable via Wails bindings
  }

  deleteSchedule(schedule: ScheduleEntry) {
    this.schedules = this.schedules.filter((s) => s.id !== schedule.id);
    this.cdr.detectChanges();
    // TODO: Call SchedulerService.Delete via Wails bindings
  }
}
