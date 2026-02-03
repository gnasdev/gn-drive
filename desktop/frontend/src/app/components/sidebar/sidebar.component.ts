import { CommonModule } from "@angular/common";
import {
  ChangeDetectionStrategy,
  Component,
  HostListener,
} from "@angular/core";
import {
  NavigationService,
  PageName,
} from "../../navigation.service.js";

interface SidebarItem {
  page: PageName;
  label: string;
  icon: string;
  section?: string;
}

@Component({
  selector: "app-sidebar",
  standalone: true,
  imports: [CommonModule],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <aside
      class="flex flex-col h-screen bg-gray-950 border-r border-gray-800 transition-all duration-200"
      [style.width]="collapsed ? '3.5rem' : '13rem'"
    >
      <!-- Logo -->
      <div class="flex items-center h-14 px-3 border-b border-gray-800">
        <div class="flex items-center gap-2 min-w-0">
          <span class="text-primary-400 font-bold text-lg shrink-0">NS</span>
          @if (!collapsed) {
          <span class="text-gray-200 font-semibold text-sm truncate"
            >Drive</span
          >
          }
        </div>
      </div>

      <!-- Navigation -->
      <nav class="flex-1 py-2 px-2 space-y-0.5 overflow-y-auto">
        @for (item of menuItems; track item.page) { @if (item.section &&
        showSection(item)) {
        <div
          class="text-[10px] uppercase tracking-wider text-gray-500 px-3 pt-3 pb-1"
        >
          @if (!collapsed) {
          {{ item.section }}
          } @else {
          <div class="border-t border-gray-700 mx-1 mt-1"></div>
          }
        </div>
        }
        <button
          (click)="navigate(item.page)"
          [class]="
            isActive(item.page)
              ? 'flex items-center gap-2 w-full px-3 py-2 rounded-lg text-primary-400 bg-primary-400/10 cursor-pointer'
              : 'flex items-center gap-2 w-full px-3 py-2 rounded-lg text-gray-400 hover:text-gray-200 hover:bg-gray-800 cursor-pointer transition-colors'
          "
          [title]="item.label"
        >
          <i [class]="item.icon" class="w-5 h-5 shrink-0"></i>
          @if (!collapsed) {
          <span class="text-sm truncate">{{ item.label }}</span>
          }
        </button>
        }
      </nav>
    </aside>
  `,
})
export class SidebarComponent {
  collapsed = false;

  readonly menuItems: SidebarItem[] = [
    { page: "dashboard", label: "Dashboard", icon: "pi pi-th-large" },
    { page: "operations", label: "Operations", icon: "pi pi-play" },
    {
      page: "file-browser",
      label: "File Browser",
      icon: "pi pi-folder-open",
    },
    {
      page: "profiles",
      label: "Profiles",
      icon: "pi pi-users",
      section: "Configuration",
    },
    { page: "remotes", label: "Remotes", icon: "pi pi-cloud" },
    { page: "schedules", label: "Schedules", icon: "pi pi-calendar" },
    {
      page: "history",
      label: "History",
      icon: "pi pi-history",
      section: "Activity",
    },
    { page: "settings", label: "Settings", icon: "pi pi-cog" },
  ];

  constructor(public readonly navigationService: NavigationService) {
    this.checkWidth(window.innerWidth);
  }

  @HostListener("window:resize", ["$event"])
  onResize(event: Event) {
    this.checkWidth((event.target as Window).innerWidth);
  }

  navigate(page: PageName) {
    this.navigationService.navigateTo(page);
  }

  isActive(page: PageName): boolean {
    const current = this.navigationService.currentState.page;
    if (page === "profiles" && current === "profile-edit") return true;
    return current === page;
  }

  showSection(item: SidebarItem): boolean {
    return !!item.section;
  }

  private checkWidth(width: number) {
    this.collapsed = width < 960;
  }
}
