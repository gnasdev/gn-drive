import { Component, Input, forwardRef } from '@angular/core';
import { CommonModule } from '@angular/common';
import { ControlValueAccessor, FormsModule, NG_VALUE_ACCESSOR } from '@angular/forms';

/* eslint-disable @angular-eslint/component-selector */
@Component({
  selector: 'neo-input',
  standalone: true,
  imports: [CommonModule, FormsModule],
  providers: [
    {
      provide: NG_VALUE_ACCESSOR,
      useExisting: forwardRef(() => NeoInputComponent),
      multi: true,
    },
  ],
  template: `
    <div class="flex flex-col gap-1">
      @if (label) {
        <!-- eslint-disable-next-line @angular-eslint/template/label-has-associated-control -->
        <label class="font-bold text-sm text-sys-fg">{{ label }}</label>
      }
      <input
        [type]="type"
        [placeholder]="placeholder"
        [disabled]="disabled"
        [(ngModel)]="value"
        (ngModelChange)="onValueChange($event)"
        (blur)="onTouched()"
        [class]="inputClasses"
      />
      @if (error) {
        <span class="text-sm text-sys-status-error font-medium">{{ error }}</span>
      }
    </div>
  `,
})
export class NeoInputComponent implements ControlValueAccessor {
  @Input() label?: string;
  @Input() placeholder = '';
  @Input() type: 'text' | 'password' | 'email' | 'number' = 'text';
  @Input() disabled = false;
  @Input() error?: string;

  value = '';
  onChange: (value: string) => void = () => { /* noop */ };
  onTouched: () => void = () => { /* noop */ };

  get inputClasses(): string {
    const base = `
      w-full px-4 py-2
      bg-sys-bg
      border-2 border-sys-border
      shadow-neo-sm
      font-medium
      placeholder:text-sys-fg-tertiary
      focus:outline-none focus:ring-2 focus:ring-sys-accent-secondary focus:ring-offset-2
      disabled:opacity-50 disabled:cursor-not-allowed
    `;
    const errorClass = this.error ? 'border-sys-status-error' : '';
    return `${base} ${errorClass}`;
  }

  onValueChange(value: string): void {
    this.value = value;
    this.onChange(value);
  }

  writeValue(value: string): void {
    this.value = value || '';
  }

  registerOnChange(fn: (value: string) => void): void {
    this.onChange = fn;
  }

  registerOnTouched(fn: () => void): void {
    this.onTouched = fn;
  }

  setDisabledState(isDisabled: boolean): void {
    this.disabled = isDisabled;
  }
}
