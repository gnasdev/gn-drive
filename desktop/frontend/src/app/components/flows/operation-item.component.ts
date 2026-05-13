/** GN Drive note: Renders reusable flow controls for the main sync workspace. */
import { ChangeDetectionStrategy, ChangeDetectorRef, Component, Input, Output, EventEmitter, inject } from '@angular/core';
import { CommonModule } from '@angular/common';
import { FormsModule } from '@angular/forms';
import { Operation, SyncConfig } from '../../models/flow.model';
import { NeoButtonComponent } from '../neo/neo-button.component';
import { RemoteDropdownComponent, RemoteInfo } from '../remote-dropdown/remote-dropdown.component';
import { PathBrowserComponent } from '../path-browser/path-browser.component';
import { OperationSettingsPanelComponent } from '../operations-tree/operation-settings-panel.component';
import { NeoDialogComponent } from '../neo/neo-dialog.component';

@Component({
  selector: 'app-flow-operation-item',
  standalone: true,
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [
    CommonModule,
    FormsModule,
    NeoButtonComponent,
    RemoteDropdownComponent,
    PathBrowserComponent,
    OperationSettingsPanelComponent,
    NeoDialogComponent,
  ],
  template: `
    <article
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
      <div class="grid grid-cols-[auto_auto_minmax(0,1fr)_auto] gap-3 p-3 items-center">
        <!-- Drag Handle -->
        <div
          class="cursor-move p-1 hover:bg-sys-accent/30"
          [class.pointer-events-none]="isExecuting"
          [class.opacity-50]="isExecuting"
          draggable="true"
          (dragstart)="onDragStart($event)"
          (dragend)="onDragEnd()"
        >
          <i class="pi pi-bars text-sys-fg-muted"></i>
        </div>

        <!-- Action Badge -->
        <div class="border-2 border-sys-border bg-sys-bg-secondary px-2 py-1 min-w-20 text-center">
          <div class="text-[10px] font-bold uppercase tracking-wide text-sys-fg-muted">Step {{ index + 1 }}</div>
          <div class="text-xs font-bold uppercase flex items-center justify-center gap-1">
            <i class="pi {{ getActionArrowClass() }}"></i>
            <span>{{ getActionLabel() }}</span>
          </div>
        </div>

        <!-- Route Summary -->
        <div class="min-w-0">
          <div class="text-[10px] font-bold uppercase tracking-wide text-sys-fg-muted mb-1">Route</div>
          <div class="flex items-center gap-2 min-w-0 text-sm font-bold">
            <span class="truncate" [title]="getEndpointSummary(operation.sourceRemote, operation.sourcePath)">
              {{ getEndpointSummary(operation.sourceRemote, operation.sourcePath) }}
            </span>
            <i class="pi {{ getActionArrowClass() }} text-xs text-sys-fg-muted shrink-0"></i>
            <span class="truncate" [title]="getEndpointSummary(operation.targetRemote, operation.targetPath)">
              {{ getEndpointSummary(operation.targetRemote, operation.targetPath) }}
            </span>
          </div>
          @if (operation.status && operation.status !== 'idle') {
            <span [class]="getStatusBadgeClass()">
              <i [class]="getStatusIcon() + ' mr-1'"></i>
              {{ operation.status | titlecase }}
            </span>
          }
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

      <!-- Collapsible Content -->
      @if (operation.isExpanded) {
        <div class="border-t-2 border-sys-border bg-sys-bg-secondary p-3 space-y-3">
          <div class="grid grid-cols-1 xl:grid-cols-3 gap-3">
            <section class="border-2 border-sys-border bg-sys-bg p-3">
              <div class="flex items-start justify-between gap-3">
                <div>
                  <div class="flex items-center gap-2 text-xs font-bold uppercase tracking-wide text-sys-fg-muted">
                    <i class="pi pi-sitemap text-[#2aa198]"></i>
                    <span>Route</span>
                  </div>
                  <div class="mt-2 text-sm font-bold leading-snug break-words">
                    {{ getEndpointSummary(operation.sourceRemote, operation.sourcePath) }}
                    <i class="pi {{ getActionArrowClass() }} text-xs text-sys-fg-muted mx-1"></i>
                    {{ getEndpointSummary(operation.targetRemote, operation.targetPath) }}
                  </div>
                </div>
                <neo-button
                  variant="secondary"
                  size="sm"
                  (onClick)="showRouteDialog = true"
                  [disabled]="isExecuting"
                >
                  <i class="pi pi-pencil"></i>
                </neo-button>
              </div>
            </section>

            <section class="border-2 border-sys-border bg-sys-bg p-3">
              <div class="flex items-start justify-between gap-3">
                <div>
                  <div class="flex items-center gap-2 text-xs font-bold uppercase tracking-wide text-sys-fg-muted">
                    <i class="pi pi-sliders-h text-[#268bd2]"></i>
                    <span>Operation setup</span>
                  </div>
                  <div class="mt-2 text-sm font-bold">{{ getOperationSetupSummary() }}</div>
                  <div class="mt-1 text-xs text-sys-fg-muted">{{ getRulesSummary() }}</div>
                </div>
                <neo-button
                  variant="secondary"
                  size="sm"
                  (onClick)="showSettingsDialog = true"
                  [disabled]="isExecuting"
                >
                  <i class="pi pi-pencil"></i>
                </neo-button>
              </div>
            </section>

            <section class="border-2 border-sys-border bg-sys-bg p-3">
              <div class="flex items-start justify-between gap-3">
                <div>
                  <div class="flex items-center gap-2 text-xs font-bold uppercase tracking-wide text-sys-fg-muted">
                    <i class="pi pi-gauge text-[#b58900]"></i>
                    <span>Execution controls</span>
                  </div>
                  <div class="mt-2 text-sm font-bold">{{ getExecutionSummary() }}</div>
                  <div class="mt-1 text-xs text-sys-fg-muted">{{ getSafetySummary() }}</div>
                </div>
                <neo-button
                  variant="secondary"
                  size="sm"
                  (onClick)="showSettingsDialog = true"
                  [disabled]="isExecuting"
                >
                  <i class="pi pi-pencil"></i>
                </neo-button>
              </div>
            </section>
          </div>
        </div>
      }
    </article>

    <neo-dialog
      [(visible)]="showRouteDialog"
      title="Edit Route"
      maxWidth="920px"
      width="92vw"
    >
      <section class="border-2 border-sys-border bg-sys-bg">
        <div class="px-3 py-2 border-b-2 border-sys-border border-l-4 border-l-[#2aa198] bg-sys-bg-secondary flex items-center gap-2">
          <i class="pi pi-sitemap text-xs text-[#2aa198]"></i>
          <span class="text-xs font-bold uppercase tracking-wide">Route</span>
        </div>
        <div class="p-3 grid grid-cols-1 lg:grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] gap-3 items-start">
          <div class="min-w-0">
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

          <div class="hidden lg:flex items-center justify-center pt-8">
            <i class="pi {{ getActionArrowClass() }} text-lg text-sys-fg-muted"></i>
          </div>

          <div class="min-w-0">
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
        </div>
      </section>
    </neo-dialog>

    <neo-dialog
      [(visible)]="showSettingsDialog"
      title="Edit Operation Settings"
      maxWidth="1120px"
      width="94vw"
    >
      <app-operation-settings-panel
        [config]="operation.syncConfig"
        [sourceRemote]="operation.sourceRemote"
        [targetRemote]="operation.targetRemote"
        [scheduleEnabled]="false"
        [cronExpr]="''"
        [disabled]="isExecuting"
        (configChange)="onConfigChange($event)"
      ></app-operation-settings-panel>
    </neo-dialog>
  `,
})
export class FlowOperationItemComponent {
  private readonly cdr = inject(ChangeDetectorRef);

  @Input() operation!: Operation;
  @Input() index!: number;
  @Input() totalInFlow!: number;
  @Input() flowRunning = false;
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

  showRouteDialog = false;
  showSettingsDialog = false;

  get isExecuting(): boolean {
    return this.flowRunning || this.operation.status === 'running' || this.operation.status === 'pending';
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

  getActionLabel(): string {
    switch (this.operation.syncConfig.action) {
      case 'pull':
        return 'Pull';
      case 'bi':
        return 'Bi';
      case 'bi-resync':
        return 'Resync';
      default:
        return 'Push';
    }
  }

  getEndpointSummary(remote: string, path: string): string {
    const remoteLabel = remote || 'Select remote';
    const pathLabel = path || '/';
    return `${remoteLabel}:${pathLabel}`;
  }

  getOperationSetupSummary(): string {
    const config = this.operation.syncConfig;
    const mode = this.getActionLabel();
    const dryRun = config.dryRun ? 'Dry run' : 'Live run';

    if (config.action === 'bi' || config.action === 'bi-resync') {
      return `${mode} · ${dryRun} · ${config.conflictResolution || 'newer'} wins`;
    }

    return `${mode} · ${dryRun} · delete ${config.deleteTiming || 'during'}`;
  }

  getRulesSummary(): string {
    const config = this.operation.syncConfig;
    const includeCount = config.includedPaths?.length || 0;
    const excludeCount = config.excludedPaths?.length || 0;
    const limits = [
      config.maxDepth ? `depth ${config.maxDepth}` : '',
      config.minSize ? `min ${config.minSize}` : '',
      config.maxSize ? `max ${config.maxSize}` : '',
    ].filter(Boolean);

    return `${includeCount} include · ${excludeCount} exclude${limits.length ? ` · ${limits.join(' · ')}` : ''}`;
  }

  getExecutionSummary(): string {
    const config = this.operation.syncConfig;
    const parallel = config.parallel || 8;
    const bandwidth = config.bandwidth ? `${config.bandwidth} MB/s` : 'unlimited';
    const checkFirst = config.checkFirst ? 'check first' : 'direct sync';

    return `${parallel} parallel · ${bandwidth} · ${checkFirst}`;
  }

  getSafetySummary(): string {
    const config = this.operation.syncConfig;
    const maxDelete = config.maxDelete ?? 100;
    const maxTransfer = config.maxTransfer || 'no transfer cap';
    const security = config.encryptSource || config.encryptDest ? 'encryption on' : 'encryption off';

    return `max delete ${maxDelete}% · ${maxTransfer} · ${security}`;
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
