import { Component, Input, Output, EventEmitter, forwardRef, ElementRef, HostListener, ContentChild, TemplateRef, inject } from '@angular/core';
import { CommonModule } from '@angular/common';
import { ControlValueAccessor, NG_VALUE_ACCESSOR } from '@angular/forms';

export interface DropdownOption {
  label: string;
  value: string;
  icon?: string;
  description?: string;
  disabled?: boolean;
  data?: unknown;
}

/* eslint-disable @angular-eslint/component-selector */
@Component({
  selector: 'neo-dropdown',
  standalone: true,
  imports: [CommonModule],
  providers: [
    {
      provide: NG_VALUE_ACCESSOR,
      useExisting: forwardRef(() => NeoDropdownComponent),
      multi: true,
    },
  ],
  template: `
    <div class="relative" [class.w-full]="fullWidth">
      @if (label) {
        <!-- eslint-disable-next-line @angular-eslint/template/label-has-associated-control -->
        <label class="block font-bold text-sm text-sys-fg mb-1">{{ label }}</label>
      }

      <!-- Trigger -->
      <button
        type="button"
        (click)="toggle()"
        [disabled]="disabled"
        [class]="triggerClasses"
      >
        <span class="flex-1 text-left truncate">
          @if (selectedOption) {
            @if (selectedOption.icon) {
              <i [class]="selectedOption.icon + ' mr-2'"></i>
            }
            {{ selectedOption.label }}
          } @else {
            <span class="text-sys-fg-muted">{{ placeholder }}</span>
          }
        </span>
        <i class="pi pi-chevron-down ml-2 transition-transform" [class.rotate-180]="isOpen"></i>
      </button>

      <!-- Dropdown Panel -->
      @if (isOpen) {
        <div class="absolute top-full left-0 right-0 mt-1 z-50 bg-sys-bg border-2 border-sys-border shadow-neo max-h-60 overflow-auto text-sys-fg">
          @for (option of options; track option.value) {
            <button
              type="button"
              (click)="selectOption(option)"
              [disabled]="option.disabled"
              class="w-full px-4 py-2 text-left flex items-start hover:bg-sys-accent/30 disabled:opacity-50 disabled:cursor-not-allowed text-sys-fg"
              [class.bg-sys-accent]="option.value === value"
            >
              @if (option.icon) {
                <i [class]="option.icon + ' mr-2 mt-0.5 shrink-0'"></i>
              }
              @if (optionTemplate) {
                <ng-container *ngTemplateOutlet="optionTemplate; context: { $implicit: option }"></ng-container>
              } @else {
                <div>
                  <span>{{ option.label }}</span>
                  @if (option.description) {
                    <span class="block text-xs text-sys-fg-muted font-normal mt-0.5">{{ option.description }}</span>
                  }
                </div>
              }
            </button>
          }

          <!-- Custom footer content -->
          <ng-content select="[footer]"></ng-content>
        </div>
      }
    </div>
  `,
})
export class NeoDropdownComponent implements ControlValueAccessor {
  private readonly elementRef = inject(ElementRef);

  @Input() options: DropdownOption[] = [];
  @Input() label?: string;
  @Input() placeholder = 'Select...';
  @Input() disabled = false;
  @Input() fullWidth = false;

  // eslint-disable-next-line @angular-eslint/no-output-on-prefix
  @Output() onSelect = new EventEmitter<DropdownOption>();

  @ContentChild('optionTemplate') optionTemplate?: TemplateRef<unknown>;

  isOpen = false;
  value = '';
  onChange: (value: string) => void = () => { /* noop */ };
  onTouched: () => void = () => { /* noop */ };

  get selectedOption(): DropdownOption | undefined {
    return this.options.find((o) => o.value === this.value);
  }

  readonly triggerClasses = `
      w-full px-4 py-2
      bg-sys-bg
      border-2 border-sys-border
      shadow-neo
      font-medium
      flex items-center
      disabled:opacity-50 disabled:cursor-not-allowed
      hover:bg-sys-accent/10
      text-sys-fg
    `;

  @HostListener('document:click', ['$event'])
  onDocumentClick(event: MouseEvent): void {
    if (!this.elementRef.nativeElement.contains(event.target)) {
      this.isOpen = false;
    }
  }

  toggle(): void {
    if (!this.disabled) {
      this.isOpen = !this.isOpen;
    }
  }

  selectOption(option: DropdownOption): void {
    if (option.disabled) return;
    this.value = option.value;
    this.onChange(this.value);
    this.onSelect.emit(option);
    this.isOpen = false;
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

  open(): void {
    this.isOpen = true;
  }

  close(): void {
    this.isOpen = false;
  }
}
