import { CommonModule } from "@angular/common";
import {
  ChangeDetectionStrategy,
  ChangeDetectorRef,
  Component,
  OnInit,
  OnDestroy,
} from "@angular/core";
import { FormsModule } from "@angular/forms";
import { BehaviorSubject, combineLatest, Subscription } from "rxjs";
import { AppService } from "../app.service";
import { ErrorService } from "../services/error.service";
import { Dialog } from "primeng/dialog";
import { ConfirmationService } from "primeng/api";

// Type imports
import {
  RemoteFormData,
  RemoteTypeOption,
  REMOTE_TYPE_OPTIONS,
} from "./remotes.types";
import { LucideAngularModule, Cloud, Plus, X, Trash2 } from "lucide-angular";

@Component({
  selector: "app-remotes",
  imports: [CommonModule, FormsModule, LucideAngularModule, Dialog],
  templateUrl: "./remotes.component.html",
  styleUrl: "./remotes.component.css",
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class RemotesComponent implements OnInit, OnDestroy {
  Date = Date;

  // Lucide Icons
  readonly CloudIcon = Cloud;
  readonly PlusIcon = Plus;
  readonly XIcon = X;
  readonly Trash2Icon = Trash2;
  private subscriptions = new Subscription();
  readonly isAddingRemote$ = new BehaviorSubject<boolean>(false);

  saveBtnText$ = new BehaviorSubject<string>("Save ✓");

  // Remote type options for the UI
  readonly remoteTypeOptions: RemoteTypeOption[] = REMOTE_TYPE_OPTIONS;

  // Modal state management
  showAddRemoteModal = false;
  addRemoteData: RemoteFormData = { name: "", type: "drive" };

  constructor(
    public readonly appService: AppService,
    private readonly cdr: ChangeDetectorRef,
    private readonly errorService: ErrorService,
    private readonly confirmationService: ConfirmationService
  ) {}

  ngOnInit(): void {
    this.subscriptions.add(
      combineLatest([
        this.appService.configInfo$,
        this.appService.remotes$,
      ]).subscribe(() => this.cdr.detectChanges())
    );
    this.appService.getConfigInfo();
  }

  ngOnDestroy(): void {
    this.subscriptions.unsubscribe();
  }

  async addRemote(e: SubmitEvent): Promise<void> {
    if (this.isAddingRemote$.value) return;

    try {
      this.isAddingRemote$.next(true);

      // get values from form and convert to object
      const form = e.target as HTMLFormElement;
      const data = new FormData(form);
      const objData: Record<string, string> = {};

      data.forEach((value, key) => {
        objData[key] = value.toString();
      });

      await this.appService.addRemote(objData);
      const parentElement = form.parentElement as HTMLDialogElement;
      parentElement.hidePopover();
    } catch (error) {
      console.error("Error adding remote:", error);
      this.errorService.handleApiError(error, "add_remote_form");
    } finally {
      this.isAddingRemote$.next(false);
    }
  }

  stopAddingRemote(): void {
    this.appService.stopAddingRemote();
  }

  saveConfigInfo(): void {
    this.appService.saveConfigInfo();
    this.saveBtnText$.next("Saved ~");
    setTimeout(() => this.saveBtnText$.next("Save ✓"), 1000);
    this.cdr.detectChanges();
  }

  // Modal methods
  openAddRemoteDialog(): void {
    this.addRemoteData = { name: "", type: "drive" };
    this.showAddRemoteModal = true;
  }

  closeAddRemoteModal(): void {
    this.showAddRemoteModal = false;
    this.addRemoteData = { name: "", type: "drive" };
  }

  async saveRemote(): Promise<void> {
    if (!this.addRemoteData.name.trim()) {
      return;
    }

    this.isAddingRemote$.next(true);
    this.cdr.detectChanges();

    try {
      await this.appService.addRemote({
        name: this.addRemoteData.name,
        type: this.addRemoteData.type,
      });
      console.log(`Remote "${this.addRemoteData.name}" added successfully!`);
      this.errorService.showSuccess(
        `Remote "${this.addRemoteData.name}" added successfully!`
      );
      this.closeAddRemoteModal();
    } catch (error: unknown) {
      console.error("Error adding remote:", error);
      this.errorService.handleApiError(error, "add_remote_modal");
    } finally {
      this.isAddingRemote$.next(false);
      this.cdr.detectChanges();
    }
  }

  confirmDeleteRemote(remote: { name: string; type: string }): void {
    const profileCount = this.getProfilesUsingRemote(remote.name);
    let message = `Are you sure you want to delete remote "${remote.name}"?`;
    if (profileCount > 0) {
      message += ` This will also affect ${profileCount} profile${profileCount === 1 ? "" : "s"} that use this remote.`;
    }

    this.confirmationService.confirm({
      message,
      header: "Confirm Delete",
      acceptButtonStyleClass:
        "bg-red-600 hover:bg-red-700 text-white font-medium py-2 px-4 rounded-lg transition-colors ml-2",
      rejectButtonStyleClass:
        "bg-gray-700 hover:bg-gray-600 text-gray-200 font-medium py-2 px-4 rounded-lg transition-colors",
      accept: () => this.executeDeleteRemote(remote.name),
    });
  }

  getProfilesUsingRemote(remoteName: string): number {
    const profiles = this.appService.configInfo$.value.profiles;
    return profiles.filter((profile) => {
      const fromRemote = profile.from.includes(":")
        ? profile.from.split(":")[0]
        : "";
      const toRemote = profile.to.includes(":") ? profile.to.split(":")[0] : "";
      return fromRemote === remoteName || toRemote === remoteName;
    }).length;
  }

  private async executeDeleteRemote(remoteName: string): Promise<void> {
    try {
      await this.appService.deleteRemote(remoteName);
      console.log(`Remote "${remoteName}" deleted successfully!`);
      this.errorService.showSuccess(
        `Remote "${remoteName}" deleted successfully!`
      );
    } catch (error) {
      console.error("Error deleting remote:", error);
      this.errorService.handleApiError(error, "delete_remote");
    } finally {
      this.cdr.detectChanges();
    }
  }

  getRemoteIcon(type: string): string {
    const option = this.remoteTypeOptions.find((opt) => opt.value === type);
    return option?.icon ?? "cloud";
  }

  getRemoteTypeLabel(type: string): string {
    const option = this.remoteTypeOptions.find((opt) => opt.value === type);
    return option?.label ?? type;
  }
}
