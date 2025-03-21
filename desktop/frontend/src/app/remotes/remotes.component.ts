import { CommonModule } from "@angular/common";
import {
  ChangeDetectionStrategy,
  ChangeDetectorRef,
  Component,
} from "@angular/core";
import { FormsModule } from "@angular/forms";
import { BehaviorSubject, combineLatest, Subscription } from "rxjs";
import { AppService } from "../app.service";

@Component({
    selector: "app-remotes",
    imports: [CommonModule, FormsModule],
    templateUrl: "./remotes.component.html",
    styleUrl: "./remotes.component.scss",
    changeDetection: ChangeDetectionStrategy.OnPush
})
export class RemotesComponent {
  Date = Date;
  private changeDetectorSub: Subscription | undefined;
  readonly isAddingRemote$ = new BehaviorSubject<boolean>(false);

  saveBtnText$ = new BehaviorSubject<string>("Save ✓");

  constructor(
    public readonly appService: AppService,
    private readonly cdr: ChangeDetectorRef
  ) {}

  ngOnInit(): void {
    this.changeDetectorSub = combineLatest([
      this.appService.configInfo$,
      this.appService.remotes$,
    ]).subscribe(() => this.cdr.detectChanges());
    this.appService.getConfigInfo();
  }

  ngOnDestroy(): void {}

  async addRemote(e: SubmitEvent) {
    if (this.isAddingRemote$.value) return;

    try {
      this.isAddingRemote$.next(true);

      // get values from form and convert to object
      const data = new FormData(e.target as HTMLFormElement);
      const objData: Record<string, string> = [
        ...(data as any).entries(),
      ].reduce((acc, [key, value]) => {
        acc[key] = value;
        return acc;
      }, {});

      await this.appService.addRemote(objData);
      (
        (e.target as HTMLFormElement).parentElement as HTMLDialogElement
      ).hidePopover();
    } catch (e) {
      alert("Error adding remote");
    } finally {
      this.isAddingRemote$.next(false);
    }
  }

  stopAddingRemote() {
    this.appService.stopAddingRemote();
  }

  deleteRemote(name: string, idk: any) {
    this.appService.deleteRemote(name);
  }

  saveConfigInfo() {
    this.appService.saveConfigInfo();
    this.saveBtnText$.next("Saved ~");
    setTimeout(() => this.saveBtnText$.next("Save ✓"), 1000);
    this.cdr.detectChanges();
  }
}
