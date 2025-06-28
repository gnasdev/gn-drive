import { CommonModule } from "@angular/common";
import {
  ChangeDetectionStrategy,
  ChangeDetectorRef,
  Component,
  OnDestroy,
  OnInit,
} from "@angular/core";
import { Action, AppService } from "./app.service.js";
import { BehaviorSubject, combineLatest, Subscription } from "rxjs";
import { HomeComponent } from "./home/home.component.js";
import { models } from "../../wailsjs/go/models.js";
import { ProfilesComponent } from "./profiles/profiles.component.js";
import { RemotesComponent } from "./remotes/remotes.component.js";

// Material Design imports
import { MatSidenavModule } from "@angular/material/sidenav";
import { MatToolbarModule } from "@angular/material/toolbar";
import { MatListModule } from "@angular/material/list";
import { MatIconModule } from "@angular/material/icon";
import { MatButtonModule } from "@angular/material/button";
import { MatTooltipModule } from "@angular/material/tooltip";
import { MatRippleModule } from "@angular/material/core";
import {
  BreakpointObserver,
  Breakpoints,
  LayoutModule,
} from "@angular/cdk/layout";

type Tab = "home" | "profiles" | "remotes";

@Component({
  selector: "app-root",
  imports: [
    CommonModule,
    HomeComponent,
    ProfilesComponent,
    RemotesComponent,
    MatSidenavModule,
    MatToolbarModule,
    MatListModule,
    MatIconModule,
    MatButtonModule,
    MatTooltipModule,
    MatRippleModule,
    LayoutModule,
  ],
  templateUrl: "./app.component.html",
  styleUrl: "./app.component.scss",
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class AppComponent implements OnInit, OnDestroy {
  Action = Action;

  readonly tab$ = new BehaviorSubject<Tab>("home");

  private changeDetectorSub: Subscription | undefined;

  // Responsive design
  isHandset = false;

  constructor(
    public readonly appService: AppService,
    private readonly cdr: ChangeDetectorRef,
    private breakpointObserver: BreakpointObserver
  ) {}

  ngOnInit() {
    // Setup responsive breakpoint observer
    this.breakpointObserver
      .observe([Breakpoints.Handset])
      .subscribe((result) => {
        this.isHandset = result.matches;
        this.cdr.detectChanges();
      });

    this.changeDetectorSub = combineLatest([
      this.appService.currentAction$,
      this.tab$,
    ]).subscribe(() => this.cdr.detectChanges());
  }

  ngOnDestroy() {}

  async pull(profile: models.Profile) {
    this.tab$.next("home");
    this.appService.pull(profile);
  }

  async push(profile: models.Profile) {
    this.tab$.next("home");
    this.appService.push(profile);
  }

  async bi(profile: models.Profile) {
    this.tab$.next("home");
    this.appService.bi(profile);
  }

  stopCommand() {
    this.appService.stopCommand();
  }

  openHome() {
    this.tab$.next("home");
  }

  openProfiles() {
    this.tab$.next("profiles");
  }

  openRemotes() {
    this.tab$.next("remotes");
  }
}
