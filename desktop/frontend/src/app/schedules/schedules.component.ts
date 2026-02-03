import { CommonModule } from "@angular/common";
import { ChangeDetectionStrategy, ChangeDetectorRef, Component } from "@angular/core";
import { FormsModule } from "@angular/forms";
import { Card } from "primeng/card";
import { Toolbar } from "primeng/toolbar";
import { Tag } from "primeng/tag";
import { ButtonModule } from "primeng/button";
import { InputText } from "primeng/inputtext";
import { Select } from "primeng/select";
import { ToggleSwitch } from "primeng/toggleswitch";
import { Dialog } from "primeng/dialog";

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
  imports: [
    CommonModule,
    FormsModule,
    Card,
    Toolbar,
    Tag,
    ButtonModule,
    InputText,
    Select,
    ToggleSwitch,
    Dialog,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="flex flex-col h-full">
      <!-- Header -->
      <p-toolbar>
        <ng-template #start>
          <div class="flex items-center gap-3">
            <i class="pi pi-calendar text-primary-400"></i>
            <h1 class="text-lg font-semibold text-gray-100">Schedules</h1>
            <span class="text-sm text-gray-500"
              >{{ schedules.length }} schedule{{
                schedules.length !== 1 ? "s" : ""
              }}</span
            >
          </div>
        </ng-template>
        <ng-template #end>
          <p-button
            size="small"
            (onClick)="showAddModal = true"
          >
            <i class="pi pi-plus"></i>
            <span>Add Schedule</span>
          </p-button>
        </ng-template>
      </p-toolbar>

      <!-- Schedule list -->
      <div class="flex-1 overflow-auto p-4">
        @if (schedules.length === 0) {
        <div
          class="flex flex-col items-center justify-center h-48 text-gray-500"
        >
          <i class="pi pi-calendar text-5xl mb-3 opacity-30"></i>
          <p class="text-sm mb-3">No schedules configured</p>
          <p-button
            size="small"
            (onClick)="showAddModal = true"
          >
            <span>Create First Schedule</span>
          </p-button>
        </div>
        } @else { @for (schedule of schedules; track schedule.id) {
        <p-card styleClass="mb-3">
          <div class="flex items-center justify-between">
            <div class="flex-1 min-w-0">
              <div class="flex items-center gap-2 mb-1">
                <span class="text-gray-200 font-medium">{{
                  schedule.profile_name
                }}</span>
                <p-tag
                  [value]="schedule.action"
                  severity="secondary"
                ></p-tag>
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
              <p-toggleswitch
                [ngModel]="schedule.enabled"
                (onChange)="toggleSchedule(schedule)"
              ></p-toggleswitch>
              <p-button
                [text]="true"
                severity="secondary"
                size="small"
                title="Run Now"
              >
                <i class="pi pi-play text-gray-400"></i>
              </p-button>
              <p-button
                [text]="true"
                severity="danger"
                size="small"
                title="Delete"
                (onClick)="deleteSchedule(schedule)"
              >
                <i class="pi pi-trash text-gray-400 hover:text-red-400"></i>
              </p-button>
            </div>
          </div>
        </p-card>
        } }
      </div>

      <!-- Add Schedule Modal -->
      <p-dialog
        header="Add Schedule"
        [(visible)]="showAddModal"
        [modal]="true"
        [style]="{ width: '24rem' }"
        [closable]="true"
      >
        <div class="space-y-3">
          <div>
            <label class="text-sm text-gray-400">Profile Name</label>
            <input
              type="text"
              pInputText
              [(ngModel)]="newSchedule.profile_name"
              class="w-full mt-1"
              placeholder="my-backup"
            />
          </div>
          <div>
            <label class="text-sm text-gray-400">Action</label>
            <p-select
              [(ngModel)]="newSchedule.action"
              [options]="actionOptions"
              optionLabel="label"
              optionValue="value"
              styleClass="w-full mt-1"
            ></p-select>
          </div>
          <div>
            <label class="text-sm text-gray-400">Cron Expression</label>
            <input
              type="text"
              pInputText
              [(ngModel)]="newSchedule.cron_expr"
              class="w-full mt-1"
              placeholder="0 */6 * * *"
            />
            <p class="text-xs text-gray-500 mt-1">
              Standard 5-field cron: min hour dom month dow
            </p>
          </div>
        </div>
        <div class="flex justify-end gap-2 mt-5">
          <p-button
            severity="secondary"
            size="small"
            [outlined]="true"
            (onClick)="showAddModal = false"
          >
            <span>Cancel</span>
          </p-button>
          <p-button
            size="small"
            (onClick)="addSchedule()"
          >
            <span>Add</span>
          </p-button>
        </div>
      </p-dialog>
    </div>
  `,
})
export class SchedulesComponent {
  schedules: ScheduleEntry[] = [];
  showAddModal = false;
  newSchedule = { profile_name: "", action: "push", cron_expr: "" };

  actionOptions = [
    { label: "Pull", value: "pull" },
    { label: "Push", value: "push" },
    { label: "Bi-Sync", value: "bi" },
    { label: "Bi-Resync", value: "bi-resync" },
  ];

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
