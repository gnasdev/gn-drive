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
import {
  LucideAngularModule,
  LayoutDashboard,
  Play,
  FolderTree,
  Users,
  Cloud,
  Calendar,
  History,
  Settings,
} from "lucide-angular";

interface SidebarItem {
  page: PageName;
  label: string;
  icon: any;
  section?: string;
}

@Component({
  selector: "app-sidebar",
  standalone: true,
  imports: [CommonModule, LucideAngularModule],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <aside
      class="sidebar"
      [class.sidebar-collapsed]="collapsed"
      [class.sidebar-expanded]="!collapsed"
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
            isActive(item.page) ? 'sidebar-item sidebar-item-active' : 'sidebar-item'
          "
          [title]="item.label"
        >
          <lucide-icon
            [img]="item.icon"
            class="w-5 h-5 shrink-0"
          ></lucide-icon>
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
    { page: "dashboard", label: "Dashboard", icon: LayoutDashboard },
    { page: "operations", label: "Operations", icon: Play },
    {
      page: "file-browser",
      label: "File Browser",
      icon: FolderTree,
    },
    {
      page: "profiles",
      label: "Profiles",
      icon: Users,
      section: "Configuration",
    },
    { page: "remotes", label: "Remotes", icon: Cloud },
    { page: "schedules", label: "Schedules", icon: Calendar },
    {
      page: "history",
      label: "History",
      icon: History,
      section: "Activity",
    },
    { page: "settings", label: "Settings", icon: Settings },
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
