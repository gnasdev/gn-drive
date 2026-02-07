import { Component, Input, Output, EventEmitter, inject } from '@angular/core';
import { CommonModule } from '@angular/common';
import { FormsModule } from '@angular/forms';
import { Operation, SyncConfig } from '../../models/operation.model';
import { NeoButtonComponent } from '../neo/neo-button.component';
import { NeoCardComponent } from '../neo/neo-card.component';
import { RemoteDropdownComponent, RemoteInfo } from '../remote-dropdown/remote-dropdown.component';
import { OperationSettingsPanelComponent } from './operation-settings-panel.component';
import { OperationLogsPanelComponent } from './operation-logs-panel.component';

@Component({
  selector: 'app-operation-item',
  standalone: true,
  imports: [
    CommonModule,
    FormsModule,
    NeoButtonComponent,
    NeoCardComponent,
    RemoteDropdownComponent,
    OperationSettingsPanelComponent,
    OperationLogsPanelComponent,
  ],
  template: `
    <div
      class="bg-sys-bg border-2 border-sys-border shadow-neo text-sys-fg"
      [class.border-sys-accent-secondary]="operation.status === 'running'"
      [class.border-sys-accent-success]="operation.status === 'completed'"
      [class.border-sys-accent-danger]="operation.status === 'failed'"
      [style.margin-left.px]="depth * 24"
      [attr.data-operation-id]="operation.id"
    >
      <!-- Main Row -->
      <div class="flex items-center gap-2 p-3">
        <!-- Drag Handle -->
        <div
          class="cursor-move p-1 hover:bg-sys-accent/30 rounded"
          draggable="true"
          (dragstart)="onDragStart($event)"
          (dragend)="onDragEnd($event)"
        >
          <i class="pi pi-bars text-sys-fg-muted"></i>
        </div>

        <!-- Expand/Collapse Toggle -->
        <neo-button
          variant="ghost"
          size="sm"
          (onClick)="toggleSettings.emit()"
        >
          <i class="pi" [class.pi-chevron-down]="operation.isExpanded" [class.pi-chevron-right]="!operation.isExpanded"></i>
        </neo-button>

        <!-- Source Remote -->
        <div class="flex-1 min-w-0">
          <app-remote-dropdown
            placeholder="Source"
            [(ngModel)]="operation.sourceRemote"
            (ngModelChange)="onOperationChange()"
            (addRemote)="onAddRemote.emit()"
            (reauthRemote)="onReauthRemote.emit($event)"
            (removeRemote)="onRemoveRemote.emit($event)"
            [disabled]="isExecuting"
          ></app-remote-dropdown>
        </div>

        <!-- Arrow -->
        <div class="flex items-center justify-center w-8">
          <i class="pi pi-arrow-right text-lg text-sys-fg" [class]="getActionArrowClass()"></i>
        </div>

        <!-- Target Remote -->
        <div class="flex-1 min-w-0">
          <app-remote-dropdown
            placeholder="Target"
            [(ngModel)]="operation.targetRemote"
            (ngModelChange)="onOperationChange()"
            (addRemote)="onAddRemote.emit()"
            (reauthRemote)="onReauthRemote.emit($event)"
            (removeRemote)="onRemoveRemote.emit($event)"
            [disabled]="isExecuting"
          ></app-remote-dropdown>
        </div>

        <!-- Action Buttons -->
        <div class="flex items-center gap-1">
          <!-- Play/Stop -->
          @if (operation.status === 'running') {
            <neo-button
              variant="danger"
              size="sm"
              (onClick)="stopOperation.emit(operation.id)"
            >
              <i class="pi pi-stop"></i>
            </neo-button>
          } @else {
            <neo-button
              variant="primary"
              size="sm"
              [disabled]="!canExecute"
              (onClick)="executeOperation.emit(operation.id)"
            >
              <i class="pi pi-play"></i>
            </neo-button>
          }

          <!-- Delete -->
          <neo-button
            variant="ghost"
            size="sm"
            (onClick)="deleteOperation.emit(operation.id)"
          >
            <i class="pi pi-trash text-sys-status-error"></i>
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
          [scheduleEnabled]="operation.scheduleEnabled"
          [cronExpr]="operation.cronExpr || ''"
          [disabled]="isExecuting"
          (configChange)="onConfigChange($event)"
          (scheduleEnabledChange)="onScheduleEnabledChange($event)"
          (cronExprChange)="onCronExprChange($event)"
        ></app-operation-settings-panel>

        <!-- Logs Panel (When executing or has logs) -->
        @if (isExecuting || operation.logs.length > 0) {
          <app-operation-logs-panel
            [logs]="operation.logs"
            [isLoading]="isExecuting && operation.logs.length === 0"
          ></app-operation-logs-panel>
        }

        <!-- Drop Zone Inside (for nesting) -->
        <div
          class="h-2 mx-3 mb-2 transition-all rounded"
          [class.h-8]="isDropTargetInside"
          [class.bg-sys-accent/30]="isDropTargetInside"
          [class.border-2]="isDropTargetInside"
          [class.border-dashed]="isDropTargetInside"
          [class.border-sys-accent]="isDropTargetInside"
          (dragover)="onDragOverInside($event)"
          (dragleave)="onDragLeaveInside($event)"
          (drop)="onDropInside($event)"
        ></div>

        <!-- Nested Operations (Children) -->
        @if (operation.children && operation.children.length > 0) {
          <div class="border-t-2 border-sys-border bg-sys-bg-secondary p-2">
            <div class="text-xs font-bold text-sys-fg-muted mb-2 px-2">
              {{ operation.groupType === 'sequential' ? 'Sequential' : 'Parallel' }} Operations
            </div>
            @for (child of operation.children; track child.id) {
              <app-operation-item
                [operation]="child"
                [depth]="0"
                (operationChange)="onChildChange($event)"
                (executeOperation)="executeOperation.emit($event)"
                (stopOperation)="stopOperation.emit($event)"
                (deleteOperation)="deleteOperation.emit($event)"
                (toggleSettings)="toggleChildSettings.emit(child.id)"
                (onAddRemote)="onAddRemote.emit()"
                (onReauthRemote)="onReauthRemote.emit($event)"
                (onRemoveRemote)="onRemoveRemote.emit($event)"
                (dragOperation)="dragOperation.emit($event)"
                (dropOperation)="dropOperation.emit($event)"
              ></app-operation-item>
            }
          </div>
        }
      }
    </div>

    <!-- Drop Zone (after this item) -->
    <div
      class="h-2 transition-all"
      [class.h-8]="isDropTarget"
      [class.bg-sys-accent-secondary/30]="isDropTarget"
      [class.border-2]="isDropTarget"
      [class.border-dashed]="isDropTarget"
      [class.border-sys-accent-secondary]="isDropTarget"
      (dragover)="onDragOver($event)"
      (dragleave)="onDragLeave($event)"
      (drop)="onDrop($event)"
    ></div>
  `,
})
export class OperationItemComponent {
  @Input() operation!: Operation;
  @Input() depth = 0;

  @Output() operationChange = new EventEmitter<Operation>();
  @Output() executeOperation = new EventEmitter<string>();
  @Output() stopOperation = new EventEmitter<string>();
  @Output() deleteOperation = new EventEmitter<string>();
  @Output() toggleSettings = new EventEmitter<void>();
  @Output() toggleChildSettings = new EventEmitter<string>();
  @Output() onAddRemote = new EventEmitter<void>();
  @Output() onReauthRemote = new EventEmitter<RemoteInfo>();
  @Output() onRemoveRemote = new EventEmitter<RemoteInfo>();
  @Output() dragOperation = new EventEmitter<{ operationId: string; event: DragEvent }>();
  @Output() dropOperation = new EventEmitter<{
    targetId: string;
    position: 'before' | 'after' | 'inside';
  }>();

  isDropTarget = false;
  isDropTargetInside = false;

  get isExecuting(): boolean {
    return this.operation.status === 'running' || this.operation.status === 'pending';
  }

  get canExecute(): boolean {
    return !!this.operation.sourceRemote && !!this.operation.targetRemote;
  }

  onOperationChange(): void {
    this.operationChange.emit(this.operation);
  }

  onExecute(): void {
    this.executeOperation.emit(this.operation.id);
  }

  onStop(): void {
    this.stopOperation.emit(this.operation.id);
  }

  onDelete(): void {
    this.deleteOperation.emit(this.operation.id);
  }

  onConfigChange(config: SyncConfig): void {
    this.operation.syncConfig = config;
    this.operationChange.emit(this.operation);
  }

  onScheduleEnabledChange(enabled: boolean): void {
    this.operation.scheduleEnabled = enabled;
    this.operationChange.emit(this.operation);
  }

  onCronExprChange(cronExpr: string): void {
    this.operation.cronExpr = cronExpr;
    this.operationChange.emit(this.operation);
  }

  onChildChange(child: Operation): void {
    // Update child in children array
    const index = this.operation.children?.findIndex((c) => c.id === child.id);
    if (index !== undefined && index >= 0 && this.operation.children) {
      this.operation.children[index] = child;
      this.operationChange.emit(this.operation);
    }
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

  // Drag and Drop
  onDragStart(event: DragEvent): void {
    event.dataTransfer?.setData('text/plain', this.operation.id);
    this.dragOperation.emit({ operationId: this.operation.id, event });
  }

  onDragEnd(event: DragEvent): void {
    // Reset any drag state
  }

  onDragOver(event: DragEvent): void {
    event.preventDefault();
    this.isDropTarget = true;
  }

  onDragLeave(event: DragEvent): void {
    this.isDropTarget = false;
  }

  onDrop(event: DragEvent): void {
    event.preventDefault();
    this.isDropTarget = false;
    const operationId = event.dataTransfer?.getData('text/plain');
    if (operationId && operationId !== this.operation.id) {
      this.dropOperation.emit({
        targetId: this.operation.id,
        position: 'after',
      });
    }
  }

  // Drop inside (nesting)
  onDragOverInside(event: DragEvent): void {
    event.preventDefault();
    event.stopPropagation();
    this.isDropTargetInside = true;
  }

  onDragLeaveInside(event: DragEvent): void {
    this.isDropTargetInside = false;
  }

  onDropInside(event: DragEvent): void {
    event.preventDefault();
    event.stopPropagation();
    this.isDropTargetInside = false;
    const operationId = event.dataTransfer?.getData('text/plain');
    if (operationId && operationId !== this.operation.id) {
      this.dropOperation.emit({
        targetId: this.operation.id,
        position: 'inside',
      });
    }
  }
}
