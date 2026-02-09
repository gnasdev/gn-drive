import { ChangeDetectorRef, Component, Input, Output, EventEmitter, inject } from '@angular/core';
import { CommonModule } from '@angular/common';
import { FormsModule } from '@angular/forms';
import { Operation, SyncConfig } from '../../models/flow.model';
import { NeoButtonComponent } from '../neo/neo-button.component';
import { RemoteDropdownComponent, RemoteInfo } from '../remote-dropdown/remote-dropdown.component';
import { PathBrowserComponent } from '../path-browser/path-browser.component';
import { OperationSettingsPanelComponent } from '../operations-tree/operation-settings-panel.component';

@Component({
  selector: 'app-flow-operation-item',
  standalone: true,
  imports: [
    CommonModule,
    FormsModule,
    NeoButtonComponent,
    RemoteDropdownComponent,
    PathBrowserComponent,
    OperationSettingsPanelComponent,
  ],
  template: `
    <div
      class="operation-item bg-sys-bg border-2 border-sys-border shadow-neo text-sys-fg transition-all"
      [class.border-sys-accent-secondary]="operation.status === 'running'"
      [class.border-sys-accent-success]="operation.status === 'completed'"
      [class.border-sys-accent-danger]="operation.status === 'failed'"
      [class.opacity-50]="willBeDragged && !isDragging"
      [class.ring-2]="willBeDragged"
      [class.ring-sys-accent]="willBeDragged"
      [class.ring-dashed]="willBeDragged"
    >
      <!-- Main Row -->
      <div class="flex items-center gap-2 p-3">
        <!-- Drag Handle -->
        <div
          class="cursor-move p-1 hover:bg-sys-accent/30 rounded"
          draggable="true"
          (dragstart)="onDragStart($event)"
          (dragend)="onDragEnd()"
        >
          <i class="pi pi-bars text-sys-fg-muted"></i>
        </div>

        <!-- Source Remote + Path -->
        <div class="flex-1 min-w-0">
          <span class="block text-xs font-bold text-sys-fg-muted mb-1">Source</span>
          <app-remote-dropdown
            placeholder="Select remote"
            [(ngModel)]="operation.sourceRemote"
            (ngModelChange)="onOperationChange()"
            (addRemote)="addRemote.emit()"
            (reauthRemote)="reauthRemote.emit($event)"
            (removeRemote)="removeRemote.emit($event)"
            [disabled]="isExecuting"
          ></app-remote-dropdown>
          <div class="mt-2">
            <app-path-browser
              [remoteName]="operation.sourceRemote"
              [(path)]="operation.sourcePath"
              (pathChange)="onOperationChange()"
              placeholder="/"
              filterMode="folder"
              [disabled]="isExecuting"
            ></app-path-browser>
          </div>
        </div>

        <!-- Arrow -->
        <div class="flex items-center justify-center w-8">
          <i class="pi text-lg text-sys-fg" [class]="getActionArrowClass()"></i>
        </div>

        <!-- Target Remote + Path -->
        <div class="flex-1 min-w-0">
          <span class="block text-xs font-bold text-sys-fg-muted mb-1">Target</span>
          <app-remote-dropdown
            placeholder="Select remote"
            [(ngModel)]="operation.targetRemote"
            (ngModelChange)="onOperationChange()"
            (addRemote)="addRemote.emit()"
            (reauthRemote)="reauthRemote.emit($event)"
            (removeRemote)="removeRemote.emit($event)"
            [disabled]="isExecuting"
          ></app-remote-dropdown>
          <div class="mt-2">
            <app-path-browser
              [remoteName]="operation.targetRemote"
              [(path)]="operation.targetPath"
              (pathChange)="onOperationChange()"
              placeholder="/"
              filterMode="folder"
              [disabled]="isExecuting"
            ></app-path-browser>
          </div>
        </div>

        <!-- Action Buttons -->
        <div class="flex items-center gap-2">
          <!-- Delete -->
          <neo-button
            variant="secondary"
            size="sm"
            (onClick)="remove.emit()"
            [disabled]="isExecuting"
          >
            <i class="pi pi-trash text-sys-status-error"></i>
          </neo-button>

          <!-- Expand/Collapse Toggle -->
          <neo-button
            variant="secondary"
            size="sm"
            (onClick)="toggleExpanded.emit()"
          >
            <i class="pi" [class.pi-chevron-down]="operation.isExpanded" [class.pi-chevron-right]="!operation.isExpanded"></i>
          </neo-button>
        </div>
      </div>

      <!-- Status Badge -->
      @if (operation.status && operation.status !== 'idle') {
        <div class="px-3 pb-2">
          <span [class]="getStatusBadgeClass()">
            <i [class]="getStatusIcon() + ' mr-1'"></i>
            {{ operation.status | titlecase }}
          </span>
        </div>
      }

      <!-- Collapsible Content -->
      @if (operation.isExpanded) {
        <!-- Settings Panel -->
        <app-operation-settings-panel
          [config]="operation.syncConfig"
          [sourceRemote]="operation.sourceRemote"
          [targetRemote]="operation.targetRemote"
          [scheduleEnabled]="false"
          [cronExpr]="''"
          [disabled]="isExecuting"
          (configChange)="onConfigChange($event)"
        ></app-operation-settings-panel>

      }
    </div>
  `,
})
export class FlowOperationItemComponent {
  private readonly cdr = inject(ChangeDetectorRef);

  @Input() operation!: Operation;
  @Input() index!: number;
  @Input() totalInFlow!: number;
  @Input() isDragging = false;
  @Input() willBeDragged = false;

  @Output() operationChange = new EventEmitter<Operation>();
  @Output() remove = new EventEmitter<void>();
  @Output() toggleExpanded = new EventEmitter<void>();
  @Output() addRemote = new EventEmitter<void>();
  @Output() reauthRemote = new EventEmitter<RemoteInfo>();
  @Output() removeRemote = new EventEmitter<RemoteInfo>();
  @Output() dragStart = new EventEmitter<{ index: number; event: DragEvent }>();
  @Output() dragEnd = new EventEmitter<void>();

  get isExecuting(): boolean {
    return this.operation.status === 'running' || this.operation.status === 'pending';
  }

  onOperationChange(): void {
    // Emit a copy to avoid shared mutation with parent state
    this.operationChange.emit({ ...this.operation });
    this.cdr.detectChanges();
  }

  onConfigChange(config: SyncConfig): void {
    // Emit a new operation with updated config (don't mutate @Input)
    this.operationChange.emit({ ...this.operation, syncConfig: config });
    this.cdr.detectChanges();
  }

  onDragStart(event: DragEvent): void {
    event.dataTransfer?.setData('text/plain', this.index.toString());
    this.dragStart.emit({ index: this.index, event });
  }

  onDragEnd(): void {
    this.dragEnd.emit();
  }

  getActionArrowClass(): string {
    const action = this.operation.syncConfig.action;
    switch (action) {
      case 'pull':
        return 'pi-arrow-left';
      case 'bi':
      case 'bi-resync':
        return 'pi-arrows-h';
      default:
        return 'pi-arrow-right';
    }
  }

  getStatusBadgeClass(): string {
    const base = 'inline-flex items-center px-2 py-0.5 text-xs font-medium rounded';
    switch (this.operation.status) {
      case 'running':
        return `${base} bg-sys-status-info-bg text-sys-status-info`;
      case 'completed':
        return `${base} bg-sys-status-success-bg text-sys-status-success`;
      case 'failed':
        return `${base} bg-sys-status-error-bg text-sys-status-error`;
      case 'pending':
        return `${base} bg-sys-status-warning-bg text-sys-status-warning`;
      case 'cancelled':
        return `${base} bg-sys-bg-tertiary text-sys-fg-muted`;
      default:
        return `${base} bg-sys-bg-secondary text-sys-fg-muted`;
    }
  }

  getStatusIcon(): string {
    switch (this.operation.status) {
      case 'running':
        return 'pi pi-spin pi-spinner';
      case 'completed':
        return 'pi pi-check-circle';
      case 'failed':
        return 'pi pi-times-circle';
      case 'pending':
        return 'pi pi-clock';
      case 'cancelled':
        return 'pi pi-ban';
      default:
        return 'pi pi-circle';
    }
  }
}
