import { Component, Input, Output, EventEmitter } from '@angular/core';
import { CommonModule } from '@angular/common';

@Component({
  selector: 'app-drop-zone',
  standalone: true,
  imports: [CommonModule],
  template: `
    <div
      class="drop-zone transition-all"
      [class.h-2]="!isActive"
      [class.h-10]="isActive"
      [class.my-3]="isActive"
      [class.opacity-0]="!isActive"
      [class.opacity-100]="isActive"
      (dragover)="onDragOver($event)"
      (dragleave)="onDragLeave($event)"
      (drop)="onDrop($event)"
    >
      <div
        class="h-full rounded transition-all flex items-center justify-center"
        [class.bg-sys-accent/20]="isActive && !isHover"
        [class.bg-sys-accent]="isHover"
        [class.border-2]="isActive"
        [class.border-dashed]="isActive && !isHover"
        [class.border-sys-accent]="isActive"
      >
        @if (isHover) {
          <span class="text-xs font-bold text-sys-fg">Drop here</span>
        }
      </div>
    </div>
  `,
})
export class DropZoneComponent {
  @Input() isActive = false;
  @Output() dropped = new EventEmitter<void>();

  isHover = false;

  onDragOver(event: DragEvent): void {
    event.preventDefault();
    event.stopPropagation();
    this.isHover = true;
  }

  onDragLeave(event: DragEvent): void {
    event.stopPropagation();
    this.isHover = false;
  }

  onDrop(event: DragEvent): void {
    event.preventDefault();
    event.stopPropagation();
    this.isHover = false;
    this.dropped.emit();
  }
}
