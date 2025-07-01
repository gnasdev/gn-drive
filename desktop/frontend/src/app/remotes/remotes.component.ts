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

// Material Design imports
import { MatCardModule } from "@angular/material/card";
import { MatButtonModule } from "@angular/material/button";
import { MatIconModule } from "@angular/material/icon";
import { MatListModule } from "@angular/material/list";
import { MatDialogModule, MatDialog } from "@angular/material/dialog";
import { MatFormFieldModule } from "@angular/material/form-field";
import { MatInputModule } from "@angular/material/input";
import { MatSelectModule } from "@angular/material/select";
import { MatProgressSpinnerModule } from "@angular/material/progress-spinner";
import { MatSnackBarModule, MatSnackBar } from "@angular/material/snack-bar";
import { MatTooltipModule } from "@angular/material/tooltip";
import { MatDialogRef, MAT_DIALOG_DATA } from "@angular/material/dialog";
import { Inject } from "@angular/core";

// Type imports
import {
  RemoteFormData,
  RemoteTypeOption,
  ConfirmDeleteDialogData,
  REMOTE_TYPE_OPTIONS,
} from "./remotes.types";

@Component({
  selector: "app-remotes",
  imports: [
    CommonModule,
    FormsModule,
    MatCardModule,
    MatButtonModule,
    MatIconModule,
    MatListModule,
    MatDialogModule,
    MatFormFieldModule,
    MatInputModule,
    MatSelectModule,
    MatProgressSpinnerModule,
    MatSnackBarModule,
    MatTooltipModule,
  ],
  templateUrl: "./remotes.component.html",
  styleUrl: "./remotes.component.css",
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class RemotesComponent implements OnInit, OnDestroy {
  Date = Date;
  private subscriptions = new Subscription();
  readonly isAddingRemote$ = new BehaviorSubject<boolean>(false);

  saveBtnText$ = new BehaviorSubject<string>("Save ✓");

  // Remote type options for the UI
  readonly remoteTypeOptions: RemoteTypeOption[] = REMOTE_TYPE_OPTIONS;

  constructor(
    public readonly appService: AppService,
    private readonly cdr: ChangeDetectorRef,
    private readonly dialog: MatDialog,
    private readonly snackBar: MatSnackBar
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
      this.snackBar.open("Error adding remote", "Close", { duration: 3000 });
    } finally {
      this.isAddingRemote$.next(false);
    }
  }

  stopAddingRemote(): void {
    this.appService.stopAddingRemote();
  }

  deleteRemote(name: string): void {
    this.appService.deleteRemote(name);
  }

  saveConfigInfo(): void {
    this.appService.saveConfigInfo();
    this.saveBtnText$.next("Saved ~");
    setTimeout(() => this.saveBtnText$.next("Save ✓"), 1000);
    this.cdr.detectChanges();
  }

  // New methods for Material Design interface
  async openAddRemoteDialog(): Promise<void> {
    const dialogRef = this.dialog.open(AddRemoteDialogComponent, {
      width: "400px",
      data: {} as RemoteFormData,
      panelClass: "dark-dialog",
    });

    dialogRef
      .afterClosed()
      .subscribe(async (result: RemoteFormData | undefined) => {
        if (result) {
          this.isAddingRemote$.next(true);
          try {
            await this.appService.addRemote({
              name: result.name,
              type: result.type,
            });
            this.snackBar.open(
              `Remote "${result.name}" added successfully!`,
              "Close",
              {
                duration: 3000,
              }
            );
          } catch (error) {
            console.error("Error adding remote:", error);
            this.snackBar.open("Error adding remote", "Close", {
              duration: 3000,
            });
          } finally {
            this.isAddingRemote$.next(false);
          }
        }
      });
  }

  confirmDeleteRemote(remote: { name: string; type: string }): void {
    console.log("Delete button clicked for remote:", remote.name);
    const dialogRef = this.dialog.open(ConfirmDeleteDialogComponent, {
      width: "350px",
      data: { remoteName: remote.name } as ConfirmDeleteDialogData,
      panelClass: "dark-dialog",
    });

    dialogRef
      .afterClosed()
      .subscribe(async (confirmed: boolean | undefined) => {
        if (confirmed) {
          try {
            await this.appService.deleteRemote(remote.name);
            this.snackBar.open(`Remote "${remote.name}" deleted`, "Close", {
              duration: 3000,
            });
          } catch (error) {
            console.error("Error deleting remote:", error);
            this.snackBar.open("Error deleting remote", "Close", {
              duration: 3000,
            });
          }
        }
      });
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

// Add Remote Dialog Component
@Component({
  selector: "app-add-remote-dialog",
  template: `
    <div>
      <h2 mat-dialog-title>Add New Remote</h2>
      <mat-dialog-content>
        <form #form="ngForm" (ngSubmit)="onSubmit()">
          <mat-form-field appearance="outline" class="full-width mt-1">
            <mat-label>Remote Name</mat-label>
            <input matInput [(ngModel)]="data.name" name="name" required />
          </mat-form-field>

          <mat-form-field appearance="outline" class="full-width">
            <mat-label>Remote Type</mat-label>
            <mat-select [(ngModel)]="data.type" name="type" required>
              <mat-option value="drive">Google Drive</mat-option>
              <mat-option value="dropbox">Dropbox</mat-option>
              <mat-option value="onedrive">OneDrive</mat-option>
              <mat-option value="yandex">Yandex Disk</mat-option>
              <mat-option value="gphotos">Google Photos</mat-option>
            </mat-select>
          </mat-form-field>
        </form>
      </mat-dialog-content>
      <mat-dialog-actions align="end">
        <button mat-button (click)="onCancel()" color="warn">Cancel</button>
        <button
          mat-raised-button
          color="primary"
          (click)="onSubmit()"
          [disabled]="!data.name || !data.type"
        >
          Add Remote
        </button>
      </mat-dialog-actions>
    </div>
  `,
  styles: [
    `
      .full-width {
        width: 100%;
      }

      :host {
        color: #ffffff;
      }

      .mat-mdc-dialog-title {
        color: #ffffff !important;
      }

      .mat-mdc-dialog-content {
        color: #ffffff !important;
      }

      .mat-mdc-form-field {
        color: #ffffff !important;
      }

      .mat-mdc-form-field .mat-mdc-floating-label {
        color: #ffffff !important;
      }

      .mat-mdc-form-field .mat-mdc-input-element {
        color: #ffffff !important;
      }

      .mat-mdc-form-field .mat-mdc-line-ripple {
        background-color: #ffffff !important;
      }

      .mat-mdc-form-field .mat-mdc-notched-outline {
        border-color: rgba(255, 255, 255, 0.3) !important;
      }

      .mat-mdc-form-field:hover .mat-mdc-notched-outline {
        border-color: rgba(255, 255, 255, 0.5) !important;
      }

      .mat-mdc-form-field.mat-focused .mat-mdc-notched-outline {
        border-color: #ffffff !important;
      }

      .mat-mdc-select-value {
        color: #ffffff !important;
      }

      .mat-mdc-select-arrow {
        color: #ffffff !important;
      }
    `,
  ],
  standalone: true,
  imports: [
    CommonModule,
    FormsModule,
    MatDialogModule,
    MatFormFieldModule,
    MatInputModule,
    MatSelectModule,
    MatButtonModule,
  ],
})
export class AddRemoteDialogComponent {
  data: RemoteFormData = { name: "", type: "drive" };

  constructor(
    public dialogRef: MatDialogRef<AddRemoteDialogComponent>,
    @Inject(MAT_DIALOG_DATA) public dialogData: RemoteFormData
  ) {
    if (dialogData) {
      this.data = { ...dialogData };
    }
  }

  onSubmit(): void {
    if (this.data.name && this.data.type) {
      this.dialogRef.close(this.data);
    }
  }

  onCancel(): void {
    this.dialogRef.close();
  }
}

// Confirm Delete Dialog Component
@Component({
  selector: "app-confirm-delete-dialog",
  template: `
    <h2 mat-dialog-title>Confirm Delete</h2>
    <mat-dialog-content>
      <p>
        Are you sure you want to delete remote
        <strong>"{{ data.remoteName }}"</strong>?
      </p>
      <p class="warning-text">This action cannot be undone.</p>
    </mat-dialog-content>
    <mat-dialog-actions align="end">
      <button mat-button (click)="onCancel()">Cancel</button>
      <button mat-raised-button color="warn" (click)="onConfirm()">
        Delete
      </button>
    </mat-dialog-actions>
  `,
  styles: [
    `
      .warning-text {
        color: #ff6b6b;
        font-size: 14px;
        margin-top: 8px;
      }

      :host {
        color: #ffffff;
      }

      .mat-mdc-dialog-title {
        color: #ffffff !important;
      }

      .mat-mdc-dialog-content {
        color: #ffffff !important;
      }

      .mat-mdc-dialog-content p {
        color: #ffffff !important;
      }

      .mat-mdc-dialog-content strong {
        color: #ffffff !important;
      }
    `,
  ],
  standalone: true,
  imports: [CommonModule, MatDialogModule, MatButtonModule],
})
export class ConfirmDeleteDialogComponent {
  constructor(
    public dialogRef: MatDialogRef<ConfirmDeleteDialogComponent>,
    @Inject(MAT_DIALOG_DATA) public data: ConfirmDeleteDialogData
  ) {}

  onConfirm(): void {
    this.dialogRef.close(true);
  }

  onCancel(): void {
    this.dialogRef.close(false);
  }
}
