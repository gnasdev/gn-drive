import { Component, Input, Output, EventEmitter } from '@angular/core';
import { CommonModule } from '@angular/common';

type ButtonVariant = 'primary' | 'secondary' | 'danger' | 'ghost';
type ButtonSize = 'sm' | 'md' | 'lg';

@Component({
  selector: 'neo-button',
  standalone: true,
  imports: [CommonModule],
  template: `
    <button
      [type]="type"
      [disabled]="disabled || loading"
      (click)="onClick.emit($event)"
      [class]="buttonClasses"
    >
      @if (loading) {
        <i class="pi pi-spinner pi-spin mr-2"></i>
      }
      <ng-content></ng-content>
    </button>
  `,
})
export class NeoButtonComponent {
  @Input() variant: ButtonVariant = 'primary';
  @Input() size: ButtonSize = 'md';
  @Input() disabled = false;
  @Input() loading = false;
  @Input() type: 'button' | 'submit' = 'button';
  @Input() fullWidth = false;

  @Output() onClick = new EventEmitter<MouseEvent>();

  get buttonClasses(): string {
    const base = `
      inline-flex items-center justify-center
      font-bold
      border-2 border-sys-border
      transition-all duration-100
      disabled:opacity-50 disabled:cursor-not-allowed disabled:translate-y-0 disabled:shadow-neo
    `;

    const variants: Record<ButtonVariant, string> = {
      primary: 'bg-sys-accent text-sys-fg shadow-neo hover:translate-y-1 hover:shadow-none active:translate-y-1 active:shadow-none',
      secondary: 'bg-sys-bg text-sys-fg shadow-neo hover:translate-y-1 hover:shadow-none active:translate-y-1 active:shadow-none',
      danger: 'bg-sys-accent-danger text-sys-fg shadow-neo hover:translate-y-1 hover:shadow-none active:translate-y-1 active:shadow-none',
      ghost: 'bg-transparent border-transparent shadow-none hover:bg-sys-fg/10',
    };

    const sizes: Record<ButtonSize, string> = {
      sm: 'px-2 py-1 text-sm',
      md: 'px-4 py-2 text-base',
      lg: 'px-6 py-3 text-lg',
    };

    const width = this.fullWidth ? 'w-full' : '';

    return `${base} ${variants[this.variant]} ${sizes[this.size]} ${width}`;
  }
}
