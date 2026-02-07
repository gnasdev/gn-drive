import {
  Component,
  Input,
  ElementRef,
  ViewChild,
  AfterViewChecked,
  OnChanges,
  SimpleChanges,
} from '@angular/core';
import { CommonModule } from '@angular/common';
import { NeoButtonComponent } from '../neo/neo-button.component';

@Component({
  selector: 'app-operation-logs-panel',
  standalone: true,
  imports: [CommonModule, NeoButtonComponent],
  template: `
    <div class="border-t-2 border-sys-border bg-sys-bg-inverse">
      <!-- Header -->
      <div class="flex items-center justify-between px-3 py-2 border-b border-sys-border-muted">
        <div class="flex items-center gap-2">
          <i class="pi pi-list text-sm text-sys-fg-tertiary"></i>
          <span class="text-xs font-medium text-sys-fg-inverse">Logs</span>
          <span class="text-xs text-sys-fg-muted">({{ logs.length }} lines)</span>
        </div>
        <div class="flex items-center gap-1">
          <neo-button variant="ghost" size="sm" (onClick)="copyLogs()">
            <i class="pi pi-copy text-xs"></i>
          </neo-button>
          <neo-button variant="ghost" size="sm" (onClick)="clearLogs()">
            <i class="pi pi-trash text-xs"></i>
          </neo-button>
        </div>
      </div>

      <!-- Log content -->
      <div
        #logContainer
        class="h-48 overflow-auto p-3 font-mono text-xs text-sys-fg-inverse whitespace-pre-wrap"
      >
        @if (isLoading) {
          <div class="flex items-center gap-2 text-sys-fg-muted">
            <i class="pi pi-spin pi-spinner"></i>
            <span>Waiting for logs...</span>
          </div>
        } @else if (logs.length === 0) {
          <div class="text-sys-fg-muted italic">No logs yet...</div>
        } @else {
          @for (line of logs; track $index) {
            <div [class]="getLineClass(line)">{{ line }}</div>
          }
        }
      </div>
    </div>
  `,
})
export class OperationLogsPanelComponent implements AfterViewChecked, OnChanges {
  @Input() logs: string[] = [];
  @Input() isLoading = false;

  @ViewChild('logContainer') logContainer!: ElementRef<HTMLDivElement>;

  private shouldAutoScroll = true;
  private prevLogsLength = 0;

  ngOnChanges(changes: SimpleChanges): void {
    if (changes['logs']) {
      // Auto-scroll when new logs are added
      if (this.logs.length > this.prevLogsLength) {
        this.shouldAutoScroll = true;
      }
      this.prevLogsLength = this.logs.length;
    }
  }

  ngAfterViewChecked(): void {
    if (this.shouldAutoScroll && this.logContainer) {
      const container = this.logContainer.nativeElement;
      container.scrollTop = container.scrollHeight;
      this.shouldAutoScroll = false;
    }
  }

  getLineClass(line: string): string {
    const lowerLine = line.toLowerCase();
    if (lowerLine.includes('error') || lowerLine.includes('failed')) {
      return 'text-sys-status-error';
    }
    if (lowerLine.includes('warning') || lowerLine.includes('warn')) {
      return 'text-sys-status-warning';
    }
    if (lowerLine.includes('transferred') || lowerLine.includes('completed')) {
      return 'text-sys-status-success';
    }
    return '';
  }

  copyLogs(): void {
    const text = this.logs.join('\n');
    navigator.clipboard.writeText(text).catch((err) => {
      console.error('Failed to copy logs:', err);
    });
  }

  clearLogs(): void {
    // This should emit an event to parent to clear logs
    // For now, just log
    console.log('Clear logs requested');
  }
}
