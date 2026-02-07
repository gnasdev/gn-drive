import { Component, Input, forwardRef } from '@angular/core';
import { CommonModule } from '@angular/common';
import { ControlValueAccessor, NG_VALUE_ACCESSOR } from '@angular/forms';

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
    <label class="inline-flex items-center gap-3 cursor-pointer" [class.opacity-50]="disabled" [class.cursor-not-allowed]="disabled">
      <button
        type="button"
        role="switch"
        [attr.aria-checked]="value"
        (click)="toggle()"
        [disabled]="disabled"
        [class]="toggleClasses"
      >
        <span
          class="absolute top-0.5 left-0.5 w-5 h-5 bg-sys-fg transition-transform duration-200"
          [class.translate-x-6]="value"
        ></span>
      </button>
      @if (label) {
        <span class="font-medium text-sys-fg">{{ label }}</span>
      }
    </label>
  `,
})
export class NeoToggleComponent implements ControlValueAccessor {
  @Input() label?: string;
  @Input() disabled = false;

  value = false;
  onChange: (value: boolean) => void = () => {};
  onTouched: () => void = () => {};

  get toggleClasses(): string {
    const base = 'relative w-12 h-6 border-2 border-sys-border transition-colors duration-200';
    const bgColor = this.value ? 'bg-sys-accent-success' : 'bg-sys-bg';
    return `${base} ${bgColor}`;
  }

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
