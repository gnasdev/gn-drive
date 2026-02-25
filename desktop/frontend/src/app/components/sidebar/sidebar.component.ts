import { CommonModule } from "@angular/common";
import {
    ChangeDetectionStrategy,
    ChangeDetectorRef,
    Component,
    HostListener,
    inject,
    OnDestroy,
    OnInit,
} from "@angular/core";
import { Subscription } from "rxjs";
import { NavigationService, PageName } from "../../navigation.service.js";

interface SidebarItem {
    page: PageName;
    label: string;
    icon: string;
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
            <div class="flex items-center justify-center h-14 px-3">
                <div class="flex items-center gap-2 min-w-0">
                    <img
                        src="assets/appicon.png"
                        alt="NG Drive"
                        class="w-8 h-8 rounded-full shrink-0"
                    />
                    @if (!collapsed) {
                        <span
                            class="text-gray-200 font-semibold text-sm truncate"
                            >NG Drive</span
                        >
                    }
                </div>
            </div>

            <!-- Navigation -->
            <nav class="flex-1 py-2 px-2 space-y-0.5 overflow-y-auto">
                @for (item of menuItems; track item.page) {
                    <button
                        (click)="navigate(item.page)"
                        [class]="
                            isActive(item.page)
                                ? 'flex items-center justify-center gap-2 w-full py-2 rounded-lg text-primary-400 bg-primary-400/10 cursor-pointer'
                                : 'flex items-center justify-center gap-2 w-full py-2 rounded-lg text-gray-400 hover:text-gray-200 hover:bg-gray-800 cursor-pointer transition-colors'
                        "
                        [title]="item.label"
                    >
                        <i [class]="item.icon" class="w-5 h-5 shrink-0"></i>
                        @if (!collapsed) {
                            <span class="text-sm truncate">{{
                                item.label
                            }}</span>
                        }
                    </button>
                }
            </nav>
        </aside>
    `,
})
export class SidebarComponent implements OnInit, OnDestroy {
    readonly navigationService = inject(NavigationService);
    private readonly cdr = inject(ChangeDetectorRef);

    collapsed = false;
    private subscription?: Subscription;

    readonly menuItems: SidebarItem[] = [
        { page: "board", label: "Board", icon: "pi pi-sitemap" },
        { page: "remotes", label: "Remotes", icon: "pi pi-cloud" },
        { page: "settings", label: "Settings", icon: "pi pi-cog" },
    ];

    constructor() {
        this.checkWidth(window.innerWidth);
    }

    ngOnInit(): void {
        this.subscription = this.navigationService.currentState$.subscribe(
            () => {
                this.cdr.markForCheck();
            },
        );
    }

    ngOnDestroy(): void {
        this.subscription?.unsubscribe();
    }

    @HostListener("window:resize", ["$event"])
    onResize(event: Event) {
        this.checkWidth((event.target as Window).innerWidth);
    }

    navigate(page: PageName) {
        this.navigationService.navigateTo(page);
    }

    isActive(page: PageName): boolean {
        return this.navigationService.currentState.page === page;
    }

    private checkWidth(width: number) {
        this.collapsed = width < 960;
    }
}
