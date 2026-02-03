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
          class="flex flex-col items-center justify-center flex-1 py-16 text-gray-500"
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
        [dismissableMask]="true"
        [style]="{ width: '80vw', minHeight: '80vh' }"
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
            <label class="text-sm text-gray-400">Frequency</label>
            @if (!useCustomCron) {
            <!-- Preset mode -->
            <p-select
              [options]="frequencyPresets"
              [ngModel]="newSchedule.cron_expr"
              (ngModelChange)="onPresetChange($event)"
              optionLabel="label"
              optionValue="value"
              styleClass="w-full mt-1"
              placeholder="Select frequency"
            ></p-select>
            <button
              class="text-xs text-primary-400 hover:text-primary-300 mt-1.5 cursor-pointer bg-transparent border-none p-0"
              (click)="useCustomCron = true; parseCronToParts(newSchedule.cron_expr)"
            >
              <i class="pi pi-pencil text-[10px] mr-1"></i>Custom expression
            </button>
            } @else {
            <!-- Custom mode: 5 cron field dropdowns -->
            <div class="grid grid-cols-5 gap-1.5 mt-1">
              <div>
                <label class="text-[10px] text-gray-500 block mb-0.5">Min</label>
                <p-select
                  [options]="minuteOptions"
                  [(ngModel)]="cronParts.minute"
                  (ngModelChange)="onCronPartChange()"
                  optionLabel="label"
                  optionValue="value"
                  styleClass="w-full"
                ></p-select>
              </div>
              <div>
                <label class="text-[10px] text-gray-500 block mb-0.5">Hour</label>
                <p-select
                  [options]="hourOptions"
                  [(ngModel)]="cronParts.hour"
                  (ngModelChange)="onCronPartChange()"
                  optionLabel="label"
                  optionValue="value"
                  styleClass="w-full"
                ></p-select>
              </div>
              <div>
                <label class="text-[10px] text-gray-500 block mb-0.5">Day</label>
                <p-select
                  [options]="domOptions"
                  [(ngModel)]="cronParts.dom"
                  (ngModelChange)="onCronPartChange()"
                  optionLabel="label"
                  optionValue="value"
                  styleClass="w-full"
                ></p-select>
              </div>
              <div>
                <label class="text-[10px] text-gray-500 block mb-0.5">Month</label>
                <p-select
                  [options]="monthOptions"
                  [(ngModel)]="cronParts.month"
                  (ngModelChange)="onCronPartChange()"
                  optionLabel="label"
                  optionValue="value"
                  styleClass="w-full"
                ></p-select>
              </div>
              <div>
                <label class="text-[10px] text-gray-500 block mb-0.5">Weekday</label>
                <p-select
                  [options]="dowOptions"
                  [(ngModel)]="cronParts.dow"
                  (ngModelChange)="onCronPartChange()"
                  optionLabel="label"
                  optionValue="value"
                  styleClass="w-full"
                ></p-select>
              </div>
            </div>
            <button
              class="text-xs text-primary-400 hover:text-primary-300 mt-1.5 cursor-pointer bg-transparent border-none p-0"
              (click)="useCustomCron = false"
            >
              <i class="pi pi-arrow-left text-[10px] mr-1"></i>Back to presets
            </button>
            }
            <!-- Preview -->
            <p class="text-xs text-gray-500 mt-1.5">
              <i class="pi pi-clock text-[10px] mr-1"></i>{{ getCronDescription() }}
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
  newSchedule = { profile_name: "", action: "push", cron_expr: "0 */6 * * *" };

  actionOptions = [
    { label: "Pull", value: "pull" },
    { label: "Push", value: "push" },
    { label: "Bi-Sync", value: "bi" },
    { label: "Bi-Resync", value: "bi-resync" },
  ];

  // Cron UI state
  useCustomCron = false;
  cronParts = { minute: "0", hour: "*/6", dom: "*", month: "*", dow: "*" };

  readonly frequencyPresets = [
    { label: "Every hour", value: "0 * * * *" },
    { label: "Every 2 hours", value: "0 */2 * * *" },
    { label: "Every 6 hours", value: "0 */6 * * *" },
    { label: "Every 12 hours", value: "0 */12 * * *" },
    { label: "Daily at midnight", value: "0 0 * * *" },
    { label: "Daily at 6 AM", value: "0 6 * * *" },
    { label: "Daily at noon", value: "0 12 * * *" },
    { label: "Weekly (Monday midnight)", value: "0 0 * * 1" },
    { label: "Monthly (1st at midnight)", value: "0 0 1 * *" },
  ];

  readonly minuteOptions = [
    { label: "Every minute (*)", value: "*" },
    { label: "0", value: "0" },
    { label: "5", value: "5" },
    { label: "10", value: "10" },
    { label: "15", value: "15" },
    { label: "20", value: "20" },
    { label: "30", value: "30" },
    { label: "45", value: "45" },
    { label: "Every 5 min (*/5)", value: "*/5" },
    { label: "Every 10 min (*/10)", value: "*/10" },
    { label: "Every 15 min (*/15)", value: "*/15" },
    { label: "Every 30 min (*/30)", value: "*/30" },
  ];

  readonly hourOptions = [
    { label: "Every hour (*)", value: "*" },
    { label: "0 (midnight)", value: "0" },
    { label: "1", value: "1" },
    { label: "2", value: "2" },
    { label: "3", value: "3" },
    { label: "4", value: "4" },
    { label: "5", value: "5" },
    { label: "6 (6 AM)", value: "6" },
    { label: "7", value: "7" },
    { label: "8", value: "8" },
    { label: "9", value: "9" },
    { label: "10", value: "10" },
    { label: "11", value: "11" },
    { label: "12 (noon)", value: "12" },
    { label: "13", value: "13" },
    { label: "14", value: "14" },
    { label: "15", value: "15" },
    { label: "16", value: "16" },
    { label: "17", value: "17" },
    { label: "18 (6 PM)", value: "18" },
    { label: "19", value: "19" },
    { label: "20", value: "20" },
    { label: "21", value: "21" },
    { label: "22", value: "22" },
    { label: "23", value: "23" },
    { label: "Every 2h (*/2)", value: "*/2" },
    { label: "Every 3h (*/3)", value: "*/3" },
    { label: "Every 6h (*/6)", value: "*/6" },
    { label: "Every 12h (*/12)", value: "*/12" },
  ];

  readonly domOptions = [
    { label: "Every day (*)", value: "*" },
    ...Array.from({ length: 31 }, (_, i) => ({
      label: String(i + 1),
      value: String(i + 1),
    })),
  ];

  readonly monthOptions = [
    { label: "Every month (*)", value: "*" },
    { label: "1 (Jan)", value: "1" },
    { label: "2 (Feb)", value: "2" },
    { label: "3 (Mar)", value: "3" },
    { label: "4 (Apr)", value: "4" },
    { label: "5 (May)", value: "5" },
    { label: "6 (Jun)", value: "6" },
    { label: "7 (Jul)", value: "7" },
    { label: "8 (Aug)", value: "8" },
    { label: "9 (Sep)", value: "9" },
    { label: "10 (Oct)", value: "10" },
    { label: "11 (Nov)", value: "11" },
    { label: "12 (Dec)", value: "12" },
  ];

  readonly dowOptions = [
    { label: "Every day (*)", value: "*" },
    { label: "Monday", value: "1" },
    { label: "Tuesday", value: "2" },
    { label: "Wednesday", value: "3" },
    { label: "Thursday", value: "4" },
    { label: "Friday", value: "5" },
    { label: "Saturday", value: "6" },
    { label: "Sunday", value: "0" },
    { label: "Weekdays (1-5)", value: "1-5" },
    { label: "Weekend (0,6)", value: "0,6" },
  ];

  constructor(private readonly cdr: ChangeDetectorRef) {}

  onPresetChange(cronExpr: string): void {
    this.newSchedule.cron_expr = cronExpr;
    this.parseCronToParts(cronExpr);
  }

  onCronPartChange(): void {
    const { minute, hour, dom, month, dow } = this.cronParts;
    this.newSchedule.cron_expr = `${minute} ${hour} ${dom} ${month} ${dow}`;
  }

  parseCronToParts(expr: string): void {
    const parts = expr.split(" ");
    if (parts.length === 5) {
      this.cronParts = {
        minute: parts[0],
        hour: parts[1],
        dom: parts[2],
        month: parts[3],
        dow: parts[4],
      };
    }
  }

  getCronDescription(): string {
    const { minute, hour, dom, month, dow } = this.cronParts;
    const parts: string[] = [];

    // Minute
    if (minute === "*") parts.push("every minute");
    else if (minute.startsWith("*/")) parts.push(`every ${minute.slice(2)} minutes`);
    else parts.push(`at minute ${minute}`);

    // Hour
    if (hour === "*") { /* every hour, already implied */ }
    else if (hour.startsWith("*/")) parts.push(`every ${hour.slice(2)} hours`);
    else parts.push(`at ${hour}:00`);

    // Day of month
    if (dom !== "*") parts.push(`on day ${dom}`);

    // Month
    if (month !== "*") {
      const monthNames = ["", "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
      const m = parseInt(month, 10);
      parts.push(`in ${m >= 1 && m <= 12 ? monthNames[m] : month}`);
    }

    // Day of week
    if (dow !== "*") {
      const dayMap: Record<string, string> = {
        "0": "Sunday", "1": "Monday", "2": "Tuesday", "3": "Wednesday",
        "4": "Thursday", "5": "Friday", "6": "Saturday",
        "1-5": "weekdays", "0,6": "weekends",
      };
      parts.push(`on ${dayMap[dow] || dow}`);
    }

    return parts.join(", ");
  }

  addSchedule() {
    // TODO: Call SchedulerService via Wails bindings
    this.showAddModal = false;
    this.newSchedule = { profile_name: "", action: "push", cron_expr: "0 */6 * * *" };
    this.useCustomCron = false;
    this.parseCronToParts(this.newSchedule.cron_expr);
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
