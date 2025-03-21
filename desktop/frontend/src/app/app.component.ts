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

type Tab = "home" | "profiles" | "remotes";

@Component({
    selector: "app-root",
    imports: [CommonModule, HomeComponent, ProfilesComponent, RemotesComponent],
    templateUrl: "./app.component.html",
    styleUrl: "./app.component.scss",
    changeDetection: ChangeDetectionStrategy.OnPush
})
export class AppComponent implements OnInit, OnDestroy {
  Action = Action;

  readonly tab$ = new BehaviorSubject<Tab>("home");

  private changeDetectorSub: Subscription | undefined;

  constructor(
    public readonly appService: AppService,
    private readonly cdr: ChangeDetectorRef
  ) {}

  ngOnInit() {
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
