import { ChangeDetectorRef, Component, Input, Output, EventEmitter, ViewChild, ElementRef, inject } from '@angular/core';
import { CommonModule } from '@angular/common';
import { FormsModule } from '@angular/forms';
import { Flow, Operation, DragData } from '../../models/flow.model';
import { SyncStatus } from '../../models/sync-status.interface';
import { NeoButtonComponent } from '../neo/neo-button.component';
import { FlowOperationItemComponent } from './operation-item.component';
import { DropZoneComponent } from './drop-zone.component';
import { OperationLogsPanelComponent } from '../operations-tree/operation-logs-panel.component';
import { RemoteInfo } from '../remote-dropdown/remote-dropdown.component';
import { ConfirmDialogComponent, ConfirmDialogData } from '../confirm-dialog/confirm-dialog.component';

@Component({
  selector: 'app-flow-card',
  standalone: true,
  imports: [
    CommonModule,
    FormsModule,
    NeoButtonComponent,
    FlowOperationItemComponent,
    DropZoneComponent,
    OperationLogsPanelComponent,
    ConfirmDialogComponent,
  ],
  template: `
    <div
      class="flow-card bg-sys-bg border-2 border-sys-border shadow-neo"
      [class.border-sys-accent-secondary]="flow.status === 'running'"
      [class.border-sys-accent-success]="flow.status === 'completed'"
      [class.border-sys-accent-danger]="flow.status === 'failed'"
    >
      <!-- Flow Header -->
      <div class="flex items-center gap-3 p-3 bg-sys-accent">
        <!-- Flow Name -->
        <div class="flex-1 min-w-0 flex items-center gap-2">
          @if (isEditing) {
            <input
              #nameInput
              type="text"
              class="flex-1 px-2 py-1 bg-sys-bg border-2 border-sys-border font-bold text-lg focus:outline-none"
              [placeholder]="'Flow ' + flowIndex"
              [(ngModel)]="editingName"
              (keydown.enter)="saveName()"
              (keydown.escape)="cancelEdit()"
            />
            <neo-button variant="primary" size="sm" (onClick)="saveName()">
              <i class="pi pi-check"></i>
            </neo-button>
            <neo-button variant="secondary" size="sm" (onClick)="cancelEdit()">
              <i class="pi pi-times"></i>
            </neo-button>
          } @else {
            <span class="font-bold text-lg truncate">{{ flow.name || 'Flow ' + flowIndex }}</span>
            <neo-button variant="secondary" size="sm" (onClick)="startEdit()">
              <i class="pi pi-pencil"></i>
            </neo-button>
          }
        </div>

        <!-- Operation Count -->
        <span class="text-sm font-medium px-2 py-0.5 bg-sys-fg/10 rounded">
          {{ flow.operations.length }} {{ flow.operations.length === 1 ? 'operation' : 'operations' }}
        </span>

        <!-- Status Badge -->
        @if (flow.status && flow.status !== 'idle') {
          <span [class]="getFlowStatusBadgeClass()">
            <i [class]="getFlowStatusIcon() + ' mr-1'"></i>
            {{ flow.status | titlecase }}
          </span>
        }

        <!-- Actions -->
        <div class="flex items-center gap-2">
          <!-- Run Flow -->
          @if (flow.status === 'running') {
            <neo-button
              variant="danger"
              size="sm"
              (onClick)="stopFlow.emit()"
            >
              <i class="pi pi-stop mr-1"></i>
              Stop
            </neo-button>
          } @else {
            <neo-button
              variant="primary"
              size="sm"
              [disabled]="flow.operations.length === 0 || !canExecute"
              (onClick)="executeFlow.emit()"
            >
              <i class="pi pi-play mr-1"></i>
              Run
            </neo-button>
          }

          <!-- Delete Flow -->
          <neo-button
            variant="secondary"
            size="sm"
            (onClick)="onDeleteFlow()"
            [disabled]="flow.status === 'running'"
          >
            <i class="pi pi-trash text-sys-status-error"></i>
          </neo-button>
        </div>
      </div>

      <!-- Flow Status Panel -->
      @if (flow.status === 'running' || activeOperationSyncStatus) {
        <app-operation-logs-panel
          [syncStatus]="activeOperationSyncStatus"
        ></app-operation-logs-panel>
      }

      <!-- Content -->
      <div class="p-3 space-y-2">
        <!-- Drop zone at top -->
        <app-drop-zone
          [isActive]="isDropZoneActive(0)"
          (dropped)="onDropAtIndex(0)"
        ></app-drop-zone>

        <!-- Operations List -->
        @for (operation of flow.operations; track operation.id; let i = $index; let last = $last) {
          <!-- Arrow between operations -->
          @if (i > 0) {
            <div class="flex justify-center py-1">
              <i class="pi pi-arrow-down text-sys-fg-muted"></i>
            </div>
          }

          <app-flow-operation-item
            [operation]="operation"
            [index]="i"
            [totalInFlow]="flow.operations.length"
            [flowRunning]="isFlowRunning"
            [isDragging]="isDragging && isSourceFlow && dragStartIndex === i"
            [willBeDragged]="isSourceFlow && dragStartIndex === i"
            (operationChange)="onOperationChange(i, $event)"
            (remove)="removeOperation.emit(operation.id)"
            (toggleExpanded)="toggleOperationExpanded.emit(operation.id)"
            (addRemote)="addRemote.emit()"
            (reauthRemote)="reauthRemote.emit($event)"
            (removeRemote)="removeRemote.emit($event)"
            (dragStart)="onOperationDragStart($event)"
            (dragEnd)="onOperationDragEnd()"
          ></app-flow-operation-item>

          <!-- Drop zone after each operation -->
          <app-drop-zone
            [isActive]="isDropZoneActive(i + 1)"
            (dropped)="onDropAtIndex(i + 1)"
          ></app-drop-zone>
        }

        <!-- Add Operation Placeholder -->
        <div
          class="mt-3 p-3 border-2 border-dashed border-sys-border-muted bg-sys-bg/50 flex items-center justify-center gap-2 cursor-pointer hover:border-sys-accent hover:bg-sys-accent/10 transition-all"
          [class.pointer-events-none]="isFlowRunning"
          [class.opacity-50]="isFlowRunning"
          tabindex="0"
          role="button"
          (click)="addOperation.emit()"
          (keydown.enter)="addOperation.emit()"
        >
          <i class="pi pi-plus text-sys-fg-tertiary"></i>
          <span class="text-sm text-sys-fg-muted font-medium">Add Operation</span>
        </div>

        <!-- Schedule Info -->
        @if (flow.scheduleEnabled && flow.cronExpr) {
          <div class="flex items-center gap-2 text-sm text-sys-fg-muted mt-2 pt-2 border-t border-sys-border-subtle">
            <i class="pi pi-clock"></i>
            <span>Schedule: {{ flow.cronExpr }}</span>
          </div>
        }
      </div>
    </div>

    <!-- Delete Flow Confirm Dialog -->
    <app-confirm-dialog
      [isOpen]="showDeleteConfirm"
      [data]="deleteConfirmData"
      (confirmed)="confirmDeleteFlow()"
      (cancelled)="showDeleteConfirm = false"
    ></app-confirm-dialog>
  `,
})
export class FlowCardComponent {
  @ViewChild('nameInput') nameInput?: ElementRef<HTMLInputElement>;
  private readonly cdr = inject(ChangeDetectorRef);

  @Input() flow!: Flow;
  @Input() flowIndex!: number;
  @Input() isDragging = false;
  @Input() dragData: DragData | null = null;

  @Output() flowChange = new EventEmitter<Flow>();
  @Output() toggleCollapsed = new EventEmitter<void>();
  @Output() executeFlow = new EventEmitter<void>();
  @Output() stopFlow = new EventEmitter<void>();
  @Output() removeFlow = new EventEmitter<void>();
  @Output() addOperation = new EventEmitter<void>();
  @Output() removeOperation = new EventEmitter<string>();
  @Output() operationChange = new EventEmitter<{ index: number; operation: Operation }>();
  @Output() toggleOperationExpanded = new EventEmitter<string>();
  @Output() addRemote = new EventEmitter<void>();
  @Output() reauthRemote = new EventEmitter<RemoteInfo>();
  @Output() removeRemote = new EventEmitter<RemoteInfo>();
  @Output() dragStart = new EventEmitter<number>();
  @Output() dragEnd = new EventEmitter<void>();
  @Output() dropAtIndex = new EventEmitter<number>();

  dragStartIndex: number | null = null;
  isEditing = false;
  editingName = '';
  showDeleteConfirm = false;
  deleteConfirmData: ConfirmDialogData = {
    title: 'Delete Flow',
    message: 'This flow has operations. Are you sure you want to delete it?',
    confirmText: 'Delete',
    confirmSeverity: 'danger',
  };

  get isSourceFlow(): boolean {
    return this.dragData?.sourceFlowId === this.flow.id;
  }

  isDropZoneActive(dropIndex: number): boolean {
    if (!this.isDragging) return false;
    if (!this.isSourceFlow) return true;
    // Same flow: show drop zones except adjacent to the dragged item (no-op positions)
    if (this.dragStartIndex === null) return false;
    return dropIndex !== this.dragStartIndex && dropIndex !== this.dragStartIndex + 1;
  }

  get isFlowRunning(): boolean {
    return this.flow.status === 'running';
  }

  get canExecute(): boolean {
    return this.flow.operations.every((op) => op.sourceRemote && op.targetRemote);
  }

  get activeOperationSyncStatus(): SyncStatus | null {
    // Prefer running operation's status
    const runningOp = this.flow.operations.find(op => op.status === 'running');
    if (runningOp?.syncStatus) return runningOp.syncStatus;

    // Otherwise show the last operation that has syncStatus data (completed/failed)
    for (let i = this.flow.operations.length - 1; i >= 0; i--) {
      if (this.flow.operations[i].syncStatus) {
        return this.flow.operations[i].syncStatus!;
      }
    }

    return null;
  }

  startEdit(): void {
    this.isEditing = true;
    this.editingName = this.flow.name || '';
    setTimeout(() => this.nameInput?.nativeElement.focus(), 0);
  }

  saveName(): void {
    this.isEditing = false;
    // Emit a new object instead of mutating the @Input
    this.flowChange.emit({ ...this.flow, name: this.editingName });
  }

  cancelEdit(): void {
    this.isEditing = false;
    this.editingName = '';
  }

  onFlowChange(): void {
    this.flowChange.emit({ ...this.flow });
  }

  onOperationChange(index: number, operation: Operation): void {
    this.operationChange.emit({ index, operation: { ...operation } });
    this.cdr.detectChanges();
  }

  onOperationDragStart(event: { index: number; event: DragEvent }): void {
    this.dragStartIndex = event.index;
    this.dragStart.emit(event.index);
  }

  onOperationDragEnd(): void {
    this.dragStartIndex = null;
    this.dragEnd.emit();
  }

  onDeleteFlow(): void {
    if (this.flow.operations.length > 0) {
      this.showDeleteConfirm = true;
    } else {
      this.removeFlow.emit();
    }
  }

  confirmDeleteFlow(): void {
    this.showDeleteConfirm = false;
    this.removeFlow.emit();
  }

  onDropAtIndex(index: number): void {
    this.dropAtIndex.emit(index);
  }

  getFlowStatusBadgeClass(): string {
    const base = 'inline-flex items-center px-2 py-0.5 text-xs font-medium rounded';
    switch (this.flow.status) {
      case 'running':
        return `${base} bg-sys-status-info-bg text-sys-status-info`;
      case 'completed':
        return `${base} bg-sys-status-success-bg text-sys-status-success`;
      case 'failed':
        return `${base} bg-sys-status-error-bg text-sys-status-error`;
      case 'cancelled':
        return `${base} bg-sys-bg-tertiary text-sys-fg-muted`;
      default:
        return `${base} bg-sys-bg-secondary text-sys-fg-muted`;
    }
  }

  getFlowStatusIcon(): string {
    switch (this.flow.status) {
      case 'running':
        return 'pi pi-spin pi-spinner';
      case 'completed':
        return 'pi pi-check-circle';
      case 'failed':
        return 'pi pi-times-circle';
      case 'cancelled':
        return 'pi pi-ban';
      default:
        return 'pi pi-circle';
    }
  }
}
