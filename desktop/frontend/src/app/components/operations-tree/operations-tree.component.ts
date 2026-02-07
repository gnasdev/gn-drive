import { Component, inject, OnInit } from '@angular/core';
import { CommonModule } from '@angular/common';
import { OperationsService } from '../../services/operations.service';
import { Operation } from '../../models/operation.model';
import { AppService } from '../../app.service';
import { OperationItemComponent } from './operation-item.component';
import { OperationPlaceholderComponent } from './operation-placeholder.component';
import { AddRemoteDialogComponent } from '../dialogs/add-remote-dialog.component';
import { RemoteInfo } from '../remote-dropdown/remote-dropdown.component';
import { NeoDialogComponent } from '../neo/neo-dialog.component';
import { NeoButtonComponent } from '../neo/neo-button.component';

@Component({
  selector: 'app-operations-tree',
  standalone: true,
  imports: [
    CommonModule,
    OperationItemComponent,
    OperationPlaceholderComponent,
    AddRemoteDialogComponent,
    NeoDialogComponent,
    NeoButtonComponent,
  ],
  template: `
    <div class="flex flex-col h-full bg-sys-bg-secondary">
      <!-- Operations List -->
      <div class="flex-1 overflow-auto p-4 space-y-2">
        @for (operation of operations; track operation.id) {
          <app-operation-item
            [operation]="operation"
            [depth]="0"
            (operationChange)="onOperationChange($event)"
            (executeOperation)="executeOperation($event)"
            (stopOperation)="stopOperation($event)"
            (deleteOperation)="deleteOperation($event)"
            (toggleSettings)="toggleSettings(operation.id)"
            (onAddRemote)="openAddRemoteDialog()"
            (onReauthRemote)="reauthRemote($event)"
            (onRemoveRemote)="confirmRemoveRemote($event)"
            (dragOperation)="onDragOperation($event)"
            (dropOperation)="onDropOperation(operation.id, $event)"
          ></app-operation-item>
        }

        <!-- Add Operation Placeholder -->
        <app-operation-placeholder
          (onAdd)="addOperation()"
        ></app-operation-placeholder>
      </div>
    </div>

    <!-- Add Remote Dialog -->
    <app-add-remote-dialog
      [(visible)]="showAddRemoteDialog"
      (created)="onRemoteCreated($event)"
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
export class OperationsTreeComponent implements OnInit {
  private readonly operationsService = inject(OperationsService);
  private readonly appService = inject(AppService);

  operations: Operation[] = [];

  // Dialog state
  showAddRemoteDialog = false;
  showRemoveConfirmDialog = false;
  remoteToRemove: RemoteInfo | null = null;
  isRemovingRemote = false;

  // Drag state
  private draggedOperationId: string | null = null;

  ngOnInit(): void {
    this.operationsService.loadOperations();
    this.operationsService.state$.subscribe((state) => {
      this.operations = state.operations;
    });
  }

  addOperation(): void {
    this.operationsService.addOperation();
  }

  onOperationChange(operation: Operation): void {
    this.operationsService.updateOperation(operation.id, operation);
  }

  executeOperation(operationId: string): void {
    this.operationsService.executeOperation(operationId);
  }

  stopOperation(operationId: string): void {
    this.operationsService.stopExecution(operationId);
  }

  deleteOperation(operationId: string): void {
    this.operationsService.removeOperation(operationId);
  }

  toggleSettings(operationId: string): void {
    this.operationsService.toggleSettings(operationId);
  }

  // Remote management
  openAddRemoteDialog(): void {
    this.showAddRemoteDialog = true;
  }

  onRemoteCreated(name: string): void {
    // Remote was already created during auth, just close dialog
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
      this.showRemoveConfirmDialog = false;
      this.remoteToRemove = null;
    } catch (err) {
      console.error('Failed to remove remote:', err);
    } finally {
      this.isRemovingRemote = false;
    }
  }

  // Drag and Drop
  onDragOperation(event: { operationId: string; event: DragEvent }): void {
    this.draggedOperationId = event.operationId;
  }

  onDropOperation(
    targetId: string,
    event: { targetId: string; position: 'before' | 'after' | 'inside' }
  ): void {
    if (this.draggedOperationId && this.draggedOperationId !== targetId) {
      this.operationsService.moveOperation(
        this.draggedOperationId,
        targetId,
        event.position
      );
    }
    this.draggedOperationId = null;
  }
}
