import { Component, Output, EventEmitter } from '@angular/core';
import { CommonModule } from '@angular/common';
import { NeoButtonComponent } from '../neo/neo-button.component';

@Component({
  selector: 'app-operation-placeholder',
  standalone: true,
  imports: [CommonModule, NeoButtonComponent],
  template: `
    <div
      class="p-6 border-2 border-dashed border-sys-border-muted bg-sys-bg/50 flex flex-col items-center justify-center gap-3 cursor-pointer hover:border-sys-border hover:bg-sys-accent/10 transition-colors"
      (click)="onAdd.emit()"
    >
      <i class="pi pi-plus-circle text-3xl text-sys-fg-tertiary"></i>
      <p class="text-sm text-sys-fg-muted font-medium">Add Operation</p>
      <p class="text-xs text-sys-fg-tertiary">Click to create a new sync operation</p>
    </div>
  `,
})
export class OperationPlaceholderComponent {
  @Output() onAdd = new EventEmitter<void>();
}
