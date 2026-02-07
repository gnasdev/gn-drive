import { Component, Input } from '@angular/core';
import { CommonModule } from '@angular/common';

@Component({
  selector: 'neo-card',
  standalone: true,
  imports: [CommonModule],
  template: `
    <div [class]="cardClasses">
      @if (title) {
        <div class="border-b-2 border-sys-border px-4 py-3 font-bold text-lg">
          {{ title }}
        </div>
      }
      <div [class]="contentClasses">
        <ng-content></ng-content>
      </div>
    </div>
  `,
})
export class NeoCardComponent {
  @Input() title?: string;
  @Input() noPadding = false;
  @Input() hoverable = false;

  get cardClasses(): string {
    const base = 'bg-sys-bg border-2 border-sys-border shadow-neo text-sys-fg';
    const hover = this.hoverable ? 'hover:translate-y-1 hover:shadow-none transition-all duration-100 cursor-pointer' : '';
    return `${base} ${hover}`;
  }

  get contentClasses(): string {
    return this.noPadding ? '' : 'p-4';
  }
}
