import {
  ChangeDetectorRef,
  Component,
  Input,
  Output,
  EventEmitter,
  inject,
  OnInit,
  OnDestroy,
  forwardRef,
  ElementRef,
  HostListener,
} from '@angular/core';
import { CommonModule } from '@angular/common';
import { ControlValueAccessor, NG_VALUE_ACCESSOR } from '@angular/forms';
import { AppService } from '../../app.service';

export interface RemoteInfo {
  name: string;
  type: string;
}

@Component({
  selector: 'app-remote-dropdown',
  standalone: true,
  imports: [CommonModule],
  providers: [
    {
      provide: NG_VALUE_ACCESSOR,
      useExisting: forwardRef(() => RemoteDropdownComponent),
      multi: true,
    },
  ],
  template: `
    <div class="relative">
      @if (label) {
        <!-- eslint-disable-next-line @angular-eslint/template/label-has-associated-control -->
        <label class="block text-sm font-bold mb-1">{{ label }}</label>
      }

      <!-- Trigger -->
      <button
        type="button"
        (click)="toggle()"
        [disabled]="disabled"
        class="w-full px-3 py-2 bg-sys-bg border-2 border-sys-border shadow-neo font-medium flex items-center gap-2 disabled:opacity-50 hover:bg-sys-accent/10 text-sys-fg"
      >
        @if (selectedRemote) {
          <i [class]="getRemoteIcon(selectedRemote.type) + ' text-lg text-sys-fg'"></i>
          <span class="flex-1 text-left truncate text-sys-fg">{{ selectedRemote.name }}</span>
        } @else {
          <i class="pi pi-cloud text-lg text-sys-fg-muted"></i>
          <span class="flex-1 text-left text-sys-fg-muted">{{ placeholder }}</span>
        }
        <i class="pi pi-chevron-down transition-transform text-sys-fg" [class.rotate-180]="isOpen"></i>
      </button>

      <!-- Dropdown Panel -->
      @if (isOpen) {
        <div class="absolute top-full left-0 right-0 mt-1 z-50 bg-sys-bg border-2 border-sys-border shadow-neo max-h-60 overflow-auto text-sys-fg">
          <!-- Local (always first, not deletable) -->
          <div
            class="flex items-center gap-2 px-3 py-2 hover:bg-sys-accent/30 cursor-pointer"
            [class.bg-sys-accent]="value === ''"
            tabindex="0"
            role="option"
            [attr.aria-selected]="value === ''"
            (click)="selectLocal()"
            (keydown.enter)="selectLocal()"
          >
            <i class="pi pi-folder text-lg text-sys-fg"></i>
            <span class="truncate text-sys-fg">Local</span>
          </div>

          <!-- Remote List -->
          @for (remote of remotes; track remote.name) {
            <div
              class="flex items-center justify-between px-3 py-2 hover:bg-sys-accent/30 cursor-pointer group"
              [class.bg-sys-accent]="remote.name === value"
              tabindex="0"
              role="option"
              [attr.aria-selected]="remote.name === value"
              (click)="selectRemote(remote)"
              (keydown.enter)="selectRemote(remote)"
            >
              <div class="flex items-center gap-2 flex-1">
                <i [class]="getRemoteIcon(remote.type) + ' text-lg text-sys-fg'"></i>
                <span class="truncate text-sys-fg">{{ remote.name }}</span>
              </div>

              <!-- Actions (show on hover) -->
              <div class="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                <button
                  type="button"
                  class="p-1 hover:bg-sys-accent-secondary/30 rounded text-sys-fg"
                  title="Re-authenticate"
                  (click)="onReauth(remote, $event)"
                >
                  <i class="pi pi-refresh text-xs"></i>
                </button>
                <button
                  type="button"
                  class="p-1 hover:bg-sys-accent-danger/30 rounded text-sys-fg"
                  title="Remove"
                  (click)="onRemove(remote, $event)"
                >
                  <i class="pi pi-trash text-xs"></i>
                </button>
              </div>
            </div>
          }

          @if (remotes.length === 0) {
            <div class="px-3 py-4 text-center text-sys-fg-muted text-sm">
              No remotes configured
            </div>
          }

          <!-- Add Remote -->
          <div
            class="flex items-center gap-2 px-3 py-2 border-t-2 border-sys-border hover:bg-sys-accent-success/30 cursor-pointer text-sys-fg"
            tabindex="0"
            role="button"
            (click)="onAddRemote()"
            (keydown.enter)="onAddRemote()"
          >
            <i class="pi pi-plus-circle text-lg"></i>
            <span class="font-medium">Add Remote</span>
          </div>
        </div>
      }
    </div>
  `,
})
export class RemoteDropdownComponent implements OnInit, OnDestroy, ControlValueAccessor {
  @Input() label?: string;
  @Input() placeholder = 'Select remote...';
  @Input() disabled = false;

  @Output() addRemote = new EventEmitter<void>();
  @Output() reauthRemote = new EventEmitter<RemoteInfo>();
  @Output() removeRemote = new EventEmitter<RemoteInfo>();

  private readonly appService = inject(AppService);
  private readonly elementRef = inject(ElementRef);
  private readonly cdr = inject(ChangeDetectorRef);
  private subscription?: ReturnType<typeof this.appService.remotes$.subscribe>;

  remotes: RemoteInfo[] = [];
  isOpen = false;
  value = '';
  onChange: (value: string) => void = () => { /* noop */ };
  onTouched: () => void = () => { /* noop */ };

  get selectedRemote(): RemoteInfo | undefined {
    if (this.value === '') return { name: 'Local', type: 'local' };
    return this.remotes.find((r) => r.name === this.value);
  }

  ngOnInit(): void {
    this.loadRemotes();
    // Subscribe to remote changes
    this.subscription = this.appService.remotes$.subscribe((remotes) => {
      this.remotes = remotes.map((r) => ({ name: r.name, type: r.type }));
      this.cdr.markForCheck();
    });
  }

  ngOnDestroy(): void {
    this.subscription?.unsubscribe();
  }

  private async loadRemotes(): Promise<void> {
    try {
      await this.appService.getRemotes();
    } catch (err) {
      console.error('Failed to load remotes:', err);
    }
  }

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

  selectLocal(): void {
    this.value = '';
    this.onChange(this.value);
    this.isOpen = false;
  }

  selectRemote(remote: RemoteInfo): void {
    this.value = remote.name;
    this.onChange(this.value);
    this.isOpen = false;
  }

  onReauth(remote: RemoteInfo, event: MouseEvent): void {
    event.stopPropagation();
    this.reauthRemote.emit(remote);
  }

  onRemove(remote: RemoteInfo, event: MouseEvent): void {
    event.stopPropagation();
    this.removeRemote.emit(remote);
  }

  onAddRemote(): void {
    this.isOpen = false;
    this.addRemote.emit();
  }

  getRemoteIcon(type: string): string {
    const icons: Record<string, string> = {
      drive: 'pi pi-google',
      dropbox: 'pi pi-box',
      onedrive: 'pi pi-microsoft',
      s3: 'pi pi-database',
      sftp: 'pi pi-server',
      local: 'pi pi-folder',
    };
    return icons[type] || 'pi pi-cloud';
  }

  // ControlValueAccessor
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
