import { Component, Input, forwardRef } from '@angular/core';
import { CommonModule } from '@angular/common';
import { ControlValueAccessor, NG_VALUE_ACCESSOR } from '@angular/forms';

/* eslint-disable @angular-eslint/component-selector */
@Component({
  selector: 'neo-toggle',
  standalone: true,
  imports: [CommonModule],
  providers: [
    {
      provide: NG_VALUE_ACCESSOR,
      useExisting: forwardRef(() => NeoToggleComponent),
      multi: true,
    },
  ],
  template: `
    <!-- eslint-disable-next-line @angular-eslint/template/label-has-associated-control -->
    <label class="inline-flex items-center gap-3 cursor-pointer" [class.opacity-50]="disabled" [class.cursor-not-allowed]="disabled">
      <button
        type="button"
        role="switch"
        [attr.aria-checked]="value"
        (click)="toggle()"
        [disabled]="disabled"
        class="relative w-10 h-6 border-2 border-sys-border shadow-neo-sm transition-colors duration-100"
        [class.bg-sys-accent-success]="value && !disabled"
        [class.bg-sys-bg]="!value && !disabled"
        [class.bg-sys-bg-tertiary]="disabled"
      >
        <!-- Knob -->
        <span
          class="absolute -top-0.5 -left-0.5 w-6 h-6 bg-sys-fg border-2 border-sys-border shadow-neo-sm transition-transform duration-100"
          [class.translate-x-4]="value"
        ></span>
      </button>
      @if (label) {
        <span class="font-bold text-sm text-sys-fg">{{ label }}</span>
      }
    </label>
  `,
})
export class NeoToggleComponent implements ControlValueAccessor {
  @Input() label?: string;
  @Input() disabled = false;

  value = false;
  onChange: (value: boolean) => void = () => { /* noop */ };
  onTouched: () => void = () => { /* noop */ };

  toggle(): void {
    if (this.disabled) return;
    this.value = !this.value;
    this.onChange(this.value);
    this.onTouched();
  }

  writeValue(value: boolean): void {
    this.value = !!value;
  }

  registerOnChange(fn: (value: boolean) => void): void {
    this.onChange = fn;
  }

  registerOnTouched(fn: () => void): void {
    this.onTouched = fn;
  }

  setDisabledState(isDisabled: boolean): void {
    this.disabled = isDisabled;
  }
}
