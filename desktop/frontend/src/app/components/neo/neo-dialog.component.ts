import { Component, Input, Output, EventEmitter, HostListener } from '@angular/core';
import { CommonModule } from '@angular/common';

@Component({
  selector: 'neo-dialog',
  standalone: true,
  imports: [CommonModule],
  template: `
    @if (visible) {
      <div
        class="fixed inset-0 z-50 flex items-center justify-center p-4"
        (click)="onBackdropClick($event)"
      >
        <!-- Backdrop -->
        <div class="absolute inset-0 bg-sys-bg-inverse/50"></div>

        <!-- Dialog -->
        <div
          class="relative bg-sys-bg border-2 border-sys-border shadow-neo-lg max-h-[90vh] overflow-auto animate-dialog-in text-sys-fg"
          [style.width]="width"
          [style.max-width]="maxWidth"
        >
          <!-- Header -->
          @if (title || showClose) {
            <div class="flex items-center justify-between border-b-2 border-sys-border px-4 py-3">
              <h2 class="font-bold text-lg">{{ title }}</h2>
              @if (showClose) {
                <button
                  (click)="close()"
                  class="w-8 h-8 flex items-center justify-center hover:bg-sys-fg/10 transition-colors"
                >
                  <i class="pi pi-times"></i>
                </button>
              }
            </div>
          }

          <!-- Content -->
          <div class="p-4">
            <ng-content></ng-content>
          </div>

          <!-- Footer -->
          <ng-content select="[footer]"></ng-content>
        </div>
      </div>
    }
  `,
  styles: [`
    @keyframes dialogIn {
      from {
        opacity: 0;
        transform: scale(0.95) translateY(-10px);
      }
      to {
        opacity: 1;
        transform: scale(1) translateY(0);
      }
    }
    .animate-dialog-in {
      animation: dialogIn 150ms ease-out;
    }
  `],
})
export class NeoDialogComponent {
  @Input() visible = false;
  @Input() title?: string;
  @Input() showClose = true;
  @Input() width = 'auto';
  @Input() maxWidth = '500px';
  @Input() closeOnEscape = true;
  @Input() closeOnBackdrop = true;

  @Output() visibleChange = new EventEmitter<boolean>();
  @Output() onClose = new EventEmitter<void>();

  @HostListener('document:keydown.escape')
  onEscapeKey(): void {
    if (this.visible && this.closeOnEscape) {
      this.close();
    }
  }

  onBackdropClick(event: MouseEvent): void {
    if (this.closeOnBackdrop && event.target === event.currentTarget) {
      this.close();
    }
  }

  close(): void {
    this.visible = false;
    this.visibleChange.emit(false);
    this.onClose.emit();
  }
}
