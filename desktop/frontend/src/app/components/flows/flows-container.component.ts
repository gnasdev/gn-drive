import { ChangeDetectorRef, Component, inject, OnDestroy, OnInit } from '@angular/core';
import { CommonModule } from '@angular/common';
import { Subscription } from 'rxjs';
import { FlowsService } from '../../services/flows.service';
import { Flow, Operation, DragData } from '../../models/flow.model';
import { AppService } from '../../app.service';
import { FlowCardComponent } from './flow-card.component';
import { AddRemoteDialogComponent } from '../dialogs/add-remote-dialog.component';
import { RemoteInfo } from '../remote-dropdown/remote-dropdown.component';
import { NeoDialogComponent } from '../neo/neo-dialog.component';
import { NeoButtonComponent } from '../neo/neo-button.component';

@Component({
  selector: 'app-flows-container',
  standalone: true,
  imports: [
    CommonModule,
    FlowCardComponent,
    AddRemoteDialogComponent,
    NeoDialogComponent,
    NeoButtonComponent,
  ],
  template: `
    <div class="flex flex-col h-full bg-sys-bg-secondary">
      <!-- Flows List -->
      <div class="flex-1 overflow-auto p-4 space-y-6">
        @for (flow of flows; track flow.id; let i = $index) {
          <app-flow-card
            [flow]="flow"
            [flowIndex]="i + 1"
            [isDragging]="isDragging"
            [dragData]="dragData"
            (flowChange)="onFlowChange(flow.id, $event)"
            (toggleCollapsed)="toggleFlowCollapsed(flow.id)"
            (executeFlow)="executeFlow(flow.id)"
            (stopFlow)="stopFlow(flow.id)"
            (removeFlow)="removeFlow(flow.id)"
            (addOperation)="addOperation(flow.id)"
            (removeOperation)="removeOperation(flow.id, $event)"
            (operationChange)="onOperationChange(flow.id, $event)"
            (toggleOperationExpanded)="toggleOperationExpanded(flow.id, $event)"
            (addRemote)="openAddRemoteDialog()"
            (reauthRemote)="reauthRemote($event)"
            (removeRemote)="confirmRemoveRemote($event)"
            (dragStart)="onDragStart(flow.id, $event)"
            (dragEnd)="onDragEnd()"
            (dropAtIndex)="onDropAtIndex(flow.id, $event)"
          ></app-flow-card>
        }

        <!-- Drop zone to create new flow -->
        @if (isDragging) {
          <div
            class="p-4 border-2 border-dashed border-sys-accent bg-sys-accent/10 flex items-center justify-center gap-2 transition-all"
            [class.bg-sys-accent/30]="isHoverNewFlow"
            (dragover)="onDragOverNewFlow($event)"
            (dragleave)="onDragLeaveNewFlow()"
            (drop)="onDropNewFlow($event)"
          >
            <i class="pi pi-plus-circle text-sys-accent"></i>
            <span class="text-sm font-bold text-sys-fg">Drop here to create new flow</span>
          </div>
        }

        <!-- Add Flow Placeholder -->
        <div
          class="mt-6 p-3 border-2 border-dashed border-sys-border-muted bg-sys-bg/50 flex items-center justify-center gap-2 cursor-pointer hover:border-sys-accent hover:bg-sys-accent/10 transition-all"
          tabindex="0"
          role="button"
          (click)="addFlow()"
          (keydown.enter)="addFlow()"
        >
          <i class="pi pi-plus text-sys-fg-tertiary"></i>
          <span class="text-sm text-sys-fg-muted font-medium">Add Flow</span>
        </div>
      </div>
    </div>

    <!-- Add Remote Dialog -->
    <app-add-remote-dialog
      [(visible)]="showAddRemoteDialog"
      (created)="onRemoteCreated()"
    ></app-add-remote-dialog>

    <!-- Remove Remote Confirmation -->
    <neo-dialog
      [(visible)]="showRemoveConfirmDialog"
      title="Remove Remote"
      maxWidth="400px"
    >
      <div class="space-y-4">
        <p>Are you sure you want to remove <strong>{{ remoteToRemove?.name }}</strong>?</p>
        <p class="text-sm text-sys-fg-muted">
          This will remove the remote configuration. Any operations using this remote will need to be updated.
        </p>
        <div class="flex justify-end gap-2">
          <neo-button variant="secondary" (onClick)="showRemoveConfirmDialog = false">
            Cancel
          </neo-button>
          <neo-button variant="danger" [loading]="isRemovingRemote" (onClick)="removeRemote()">
            Remove
          </neo-button>
        </div>
      </div>
    </neo-dialog>
  `,
})
export class FlowsContainerComponent implements OnInit, OnDestroy {
  private readonly flowsService = inject(FlowsService);
  private readonly appService = inject(AppService);
  private readonly cdr = inject(ChangeDetectorRef);
  private subscription = new Subscription();

  flows: Flow[] = [];
  isDragging = false;
  dragData: DragData | null = null;
  isHoverNewFlow = false;

  // Dialog state
  showAddRemoteDialog = false;
  showRemoveConfirmDialog = false;
  remoteToRemove: RemoteInfo | null = null;
  isRemovingRemote = false;

  ngOnInit(): void {
    this.flowsService.loadFlows();
    this.subscription.add(
      this.flowsService.state$.subscribe((state) => {
        this.flows = state.flows;
        this.isDragging = state.isDragging;
        this.dragData = state.dragData;
        this.cdr.detectChanges();
      })
    );
  }

  ngOnDestroy(): void {
    this.subscription.unsubscribe();
  }

  // Flow operations
  addFlow(): void {
    this.flowsService.addFlow();
  }

  removeFlow(flowId: string): void {
    this.flowsService.removeFlow(flowId);
  }

  onFlowChange(flowId: string, flow: Flow): void {
    this.flowsService.updateFlow(flowId, flow);
  }

  toggleFlowCollapsed(flowId: string): void {
    this.flowsService.toggleFlowCollapsed(flowId);
  }

  executeFlow(flowId: string): void {
    this.flowsService.executeFlow(flowId);
  }

  stopFlow(flowId: string): void {
    this.flowsService.stopFlow(flowId);
  }

  // Operation operations
  addOperation(flowId: string): void {
    this.flowsService.addOperation(flowId);
  }

  removeOperation(flowId: string, operationId: string): void {
    this.flowsService.removeOperation(flowId, operationId);
  }

  onOperationChange(flowId: string, event: { index: number; operation: Operation }): void {
    this.flowsService.updateOperation(flowId, event.operation.id, event.operation);
  }

  toggleOperationExpanded(flowId: string, operationId: string): void {
    this.flowsService.toggleOperationExpanded(flowId, operationId);
  }

  // Drag-drop
  onDragStart(flowId: string, startIndex: number): void {
    this.flowsService.startDrag(flowId, startIndex);
  }

  onDragEnd(): void {
    this.flowsService.endDrag();
    this.isHoverNewFlow = false;
  }

  onDropAtIndex(flowId: string, index: number): void {
    this.flowsService.moveOperations(flowId, index);
  }

  onDragOverNewFlow(event: DragEvent): void {
    event.preventDefault();
    this.isHoverNewFlow = true;
  }

  onDragLeaveNewFlow(): void {
    this.isHoverNewFlow = false;
  }

  onDropNewFlow(event: DragEvent): void {
    event.preventDefault();
    this.isHoverNewFlow = false;
    this.flowsService.moveOperationsToNewFlow();
  }

  // Remote management
  openAddRemoteDialog(): void {
    this.showAddRemoteDialog = true;
  }

  onRemoteCreated(): void {
    this.showAddRemoteDialog = false;
  }

  async reauthRemote(remote: RemoteInfo): Promise<void> {
    try {
      await this.appService.reauthRemote(remote.name);
    } catch (err) {
      console.error('Failed to re-authenticate remote:', err);
    }
  }

  confirmRemoveRemote(remote: RemoteInfo): void {
    this.remoteToRemove = remote;
    this.showRemoveConfirmDialog = true;
  }

  async removeRemote(): Promise<void> {
    if (!this.remoteToRemove) return;

    this.isRemovingRemote = true;
    try {
      await this.appService.deleteRemote(this.remoteToRemove.name);
      // Reload flows to pick up backend changes (OnRemoteDeleted clears references)
      await this.flowsService.loadFlows();
      this.showRemoveConfirmDialog = false;
      this.remoteToRemove = null;
    } catch (err) {
      console.error('Failed to remove remote:', err);
    } finally {
      this.isRemovingRemote = false;
      this.cdr.detectChanges();
    }
  }
}
