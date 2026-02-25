import { CommonModule } from '@angular/common';
import {
  ChangeDetectionStrategy,
  ChangeDetectorRef,
  Component,
  EventEmitter,
  inject,
  Input,
  OnInit,
  Output,
} from '@angular/core';
import { GetAppInfo } from '../../../../wailsjs/desktop/backend/app';
import { NeoCardComponent } from '../neo/neo-card.component';
import { NeoDialogComponent } from '../neo/neo-dialog.component';

interface AppInfo {
  name: string;
  version: string;
  commit: string;
  description: string;
}

interface EcosystemApp {
  name: string;
  description: string;
  url: string;
  icon: string;
}

@Component({
  selector: 'app-about-dialog',
  standalone: true,
  imports: [CommonModule, NeoDialogComponent, NeoCardComponent],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <neo-dialog
      [visible]="visible"
      (visibleChange)="visibleChange.emit($event)"
      title="About"
      maxWidth="500px"
      maxHeight="80vh"
      [headerYellow]="true"
    >
      <div class="space-y-4 h-full overflow-auto hide-scrollbar">
        <!-- App Info -->
        <neo-card>
          <div class="flex items-center gap-3 mb-3">
            <i class="pi pi-cloud text-2xl"></i>
            <div>
              <h2 class="font-bold text-lg">{{ appInfo?.name || 'NG Drive' }}</h2>
              <p class="text-xs text-sys-fg-muted">{{ appInfo?.description }}</p>
            </div>
          </div>
          <div class="space-y-1 text-sm">
            <div class="flex justify-between">
              <span class="text-sys-fg-muted">Version</span>
              <span class="font-mono">v{{ appInfo?.version || 'dev' }}</span>
            </div>
            @if (appInfo?.commit && appInfo!.commit !== 'unknown') {
              <div class="flex justify-between">
                <span class="text-sys-fg-muted">Commit</span>
                <span class="font-mono text-xs">{{ appInfo!.commit.slice(0, 7) }}</span>
              </div>
            }
          </div>
        </neo-card>

        <!-- NS Ecosystem -->
        <neo-card>
          <div class="flex items-center gap-2 mb-3">
            <i class="pi pi-th-large"></i>
            <h2 class="font-bold">NS Ecosystem</h2>
          </div>
          <div class="space-y-3">
            @for (app of ecosystemApps; track app.name) {
              <div class="flex items-start gap-3">
                <i [class]="app.icon + ' text-lg mt-0.5'"></i>
                <div class="flex-1">
                  <p class="text-sm font-medium">{{ app.name }}</p>
                  <p class="text-xs text-sys-fg-muted">{{ app.description }}</p>
                  <p class="text-xs text-sys-fg-muted font-mono">{{ app.url }}</p>
                </div>
              </div>
            }
          </div>
        </neo-card>

        <!-- Author -->
        <neo-card>
          <div class="flex items-center gap-2 mb-2">
            <i class="pi pi-user"></i>
            <h2 class="font-bold">Author</h2>
          </div>
          <div class="text-sm">
            <p class="font-medium">gnas.dev</p>
            <p class="text-xs text-sys-fg-muted font-mono">https://gnas.dev</p>
          </div>
        </neo-card>
      </div>
    </neo-dialog>
  `,
})
export class AboutDialogComponent implements OnInit {
  @Input() visible = false;
  @Output() visibleChange = new EventEmitter<boolean>();

  private readonly cdr = inject(ChangeDetectorRef);

  appInfo: AppInfo | null = null;

  ecosystemApps: EcosystemApp[] = [
    {
      name: 'gn-shop',
      description: 'E-commerce fashion store',
      url: 'shop.gnas.dev',
      icon: 'pi pi-shopping-bag',
    },
    {
      name: 'gn-engreel',
      description: 'Vocabulary learning app',
      url: 'engreel.gnas.dev',
      icon: 'pi pi-book',
    },
    {
      name: 'gn-money',
      description: 'Personal finance manager',
      url: 'money.gnas.dev',
      icon: 'pi pi-wallet',
    },
  ];

  async ngOnInit(): Promise<void> {
    try {
      this.appInfo = await GetAppInfo();
      this.cdr.markForCheck();
    } catch (err) {
      console.error('Failed to load app info:', err);
    }
  }
}
