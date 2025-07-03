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

// No Material imports needed anymore

// Type imports
import {
  RemoteFormData,
  RemoteTypeOption,
  REMOTE_TYPE_OPTIONS,
} from "./remotes.types";
import {
  LucideAngularModule,
  Cloud,
  Plus,
  Download,
  Upload,
  X,
  Trash2,
} from "lucide-angular";

@Component({
  selector: "app-remotes",
  imports: [CommonModule, FormsModule, LucideAngularModule],
  templateUrl: "./remotes.component.html",
  styleUrl: "./remotes.component.css",
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class RemotesComponent implements OnInit, OnDestroy {
  Date = Date;

  // Lucide Icons
  readonly CloudIcon = Cloud;
  readonly PlusIcon = Plus;
  readonly DownloadIcon = Download;
  readonly UploadIcon = Upload;
  readonly XIcon = X;
  readonly Trash2Icon = Trash2;
  private subscriptions = new Subscription();
  readonly isAddingRemote$ = new BehaviorSubject<boolean>(false);

  saveBtnText$ = new BehaviorSubject<string>("Save ✓");

  // Remote type options for the UI
  readonly remoteTypeOptions: RemoteTypeOption[] = REMOTE_TYPE_OPTIONS;

  // Modal state management
  showAddRemoteModal = false;
  showDeleteConfirmModal = false;
  remoteToDelete: any = null;
  addRemoteData: RemoteFormData = { name: "", type: "drive" };

  constructor(
    public readonly appService: AppService,
    private readonly cdr: ChangeDetectorRef
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
      this.closeAddRemoteModal();
    } catch (error: any) {
      console.error("Error adding remote:", error);

      // Show user-friendly error message
      const errorMessage = error?.message || "Unknown error occurred";

      // For iCloud setup instructions, show in a more prominent way
      if (
        errorMessage.includes(
          "iCloud Drive setup requires interactive configuration"
        )
      ) {
        alert(errorMessage);
      } else {
        alert(`Failed to add remote: ${errorMessage}`);
      }
    } finally {
      this.isAddingRemote$.next(false);
      this.cdr.detectChanges();
    }
  }

  confirmDeleteRemote(remote: { name: string; type: string }): void {
    this.remoteToDelete = remote;
    this.showDeleteConfirmModal = true;
  }

  closeDeleteConfirmModal(): void {
    this.showDeleteConfirmModal = false;
    this.remoteToDelete = null;
  }

  async deleteRemote(): Promise<void> {
    if (!this.remoteToDelete) return;

    try {
      await this.appService.deleteRemote(this.remoteToDelete.name);
      console.log(`Remote "${this.remoteToDelete.name}" deleted successfully!`);
    } catch (error) {
      console.error("Error deleting remote:", error);
    } finally {
      this.closeDeleteConfirmModal();
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

  async exportRemotes(): Promise<void> {
    try {
      await this.appService.exportRemotes();
    } catch (error) {
      console.error("Error exporting remotes:", error);
      alert("Failed to export remotes. Please try again.");
    }
  }

  async importRemotes(): Promise<void> {
    try {
      await this.appService.importRemotes();
    } catch (error) {
      console.error("Error importing remotes:", error);
      alert(
        "Failed to import remotes. Please check the file format and try again."
      );
    }
  }
}
