import { CommonModule } from "@angular/common";
import {
  ChangeDetectionStrategy,
  ChangeDetectorRef,
  Component,
  OnDestroy,
  OnInit,
} from "@angular/core";
import { combineLatest, map, Subscription } from "rxjs";
import { Action, AppService } from "../app.service";
import { models } from "../../../wailsjs/go/models";

@Component({
  selector: "app-home",
  standalone: true,
  imports: [CommonModule],
  templateUrl: "./home.component.html",
  styleUrl: "./home.component.css",
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class HomeComponent implements OnInit, OnDestroy {
  Action = Action;

  private changeDetectorSub: Subscription | undefined;

  readonly isCurrentProfileValid$ = this.appService.configInfo$.pipe(
    map((configInfo) => this.validateCurrentProfileIndex(configInfo))
  );

  constructor(
    public readonly appService: AppService,
    private readonly cdr: ChangeDetectorRef
  ) {}

  ngOnInit() {
    this.changeDetectorSub = combineLatest([
      this.appService.data$,
      this.appService.configInfo$,
      this.isCurrentProfileValid$,
      this.appService.currentAction$,
      this.appService.currentId$,
    ]).subscribe(() => this.cdr.detectChanges());
  }

  ngOnDestroy() {
    this.changeDetectorSub?.unsubscribe();
  }

  changeProfile(e: Event) {
    this.appService.configInfo$.value.selected_profile_index = parseInt(
      (e.target as HTMLSelectElement).value
    );
    this.appService.configInfo$.next(this.appService.configInfo$.value);
    this.appService.saveConfigInfo();
  }

  validateCurrentProfileIndex(configInfo: models.ConfigInfo) {
    return configInfo.profiles?.[configInfo.selected_profile_index];
  }

  pull() {
    if (!this.validateCurrentProfileIndex(this.appService.configInfo$.value))
      return;
    this.appService.pull(
      this.appService.configInfo$.value.profiles[
        this.appService.configInfo$.value.selected_profile_index
      ]
    );
  }

  push() {
    if (!this.validateCurrentProfileIndex(this.appService.configInfo$.value))
      return;
    this.appService.push(
      this.appService.configInfo$.value.profiles[
        this.appService.configInfo$.value.selected_profile_index
      ]
    );
  }

  bi() {
    if (!this.validateCurrentProfileIndex(this.appService.configInfo$.value))
      return;
    this.appService.bi(
      this.appService.configInfo$.value.profiles[
        this.appService.configInfo$.value.selected_profile_index
      ]
    );
  }

  biResync() {
    if (!this.validateCurrentProfileIndex(this.appService.configInfo$.value))
      return;
    this.appService.bi(
      this.appService.configInfo$.value.profiles[
        this.appService.configInfo$.value.selected_profile_index
      ],
      true
    );
  }

  stopCommand() {
    this.appService.stopCommand();
  }
}
