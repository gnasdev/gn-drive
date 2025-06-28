import { CommonModule } from "@angular/common";
import {
  ChangeDetectionStrategy,
  ChangeDetectorRef,
  Component,
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
  styleUrl: "./remotes.component.scss",
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class RemotesComponent {
  Date = Date;
  private changeDetectorSub: Subscription | undefined;
  readonly isAddingRemote$ = new BehaviorSubject<boolean>(false);

  saveBtnText$ = new BehaviorSubject<string>("Save ✓");

  constructor(
    public readonly appService: AppService,
    private readonly cdr: ChangeDetectorRef,
    private dialog: MatDialog,
    private snackBar: MatSnackBar
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

  deleteRemote(name: string, _idk: any) {
    this.appService.deleteRemote(name);
  }

  saveConfigInfo() {
    this.appService.saveConfigInfo();
    this.saveBtnText$.next("Saved ~");
    setTimeout(() => this.saveBtnText$.next("Save ✓"), 1000);
    this.cdr.detectChanges();
  }

  // New methods for Material Design interface
  async openAddRemoteDialog() {
    const dialogRef = this.dialog.open(AddRemoteDialogComponent, {
      width: "400px",
      data: {},
    });

    dialogRef.afterClosed().subscribe(async (result) => {
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

  confirmDeleteRemote(remote: any) {
    console.log("Delete button clicked for remote:", remote.name);
    const dialogRef = this.dialog.open(ConfirmDeleteDialogComponent, {
      width: "350px",
      data: { remoteName: remote.name },
    });

    dialogRef.afterClosed().subscribe(async (confirmed) => {
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
    switch (type) {
      case "drive":
        return "cloud";
      case "dropbox":
        return "cloud_queue";
      case "onedrive":
        return "cloud_circle";
      case "yandex":
        return "cloud_sync";
      case "gphotos":
        return "photo_library";
      default:
        return "cloud";
    }
  }

  getRemoteTypeLabel(type: string): string {
    switch (type) {
      case "drive":
        return "Google Drive";
      case "dropbox":
        return "Dropbox";
      case "onedrive":
        return "OneDrive";
      case "yandex":
        return "Yandex Disk";
      case "gphotos":
        return "Google Photos";
      default:
        return type;
    }
  }
}

// Add Remote Dialog Component
@Component({
  selector: "add-remote-dialog",
  template: `
    <h2 mat-dialog-title>Add New Remote</h2>
    <mat-dialog-content>
      <form #form="ngForm" (ngSubmit)="onSubmit()">
        <mat-form-field appearance="outline" class="full-width">
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
      <button mat-button (click)="onCancel()">Cancel</button>
      <button
        mat-raised-button
        color="primary"
        (click)="onSubmit()"
        [disabled]="!data.name || !data.type"
      >
        Add Remote
      </button>
    </mat-dialog-actions>
  `,
  styles: [
    `
      .full-width {
        width: 100%;
        margin-bottom: 16px;
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
  data = { name: "", type: "" };

  constructor(
    public dialogRef: MatDialogRef<AddRemoteDialogComponent>,
    @Inject(MAT_DIALOG_DATA) public dialogData: any
  ) {}

  onSubmit() {
    if (this.data.name && this.data.type) {
      this.dialogRef.close(this.data);
    }
  }

  onCancel() {
    this.dialogRef.close();
  }
}

// Confirm Delete Dialog Component
@Component({
  selector: "confirm-delete-dialog",
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
        color: #f44336;
        font-size: 14px;
        margin-top: 8px;
      }
    `,
  ],
  standalone: true,
  imports: [CommonModule, MatDialogModule, MatButtonModule],
})
export class ConfirmDeleteDialogComponent {
  constructor(
    public dialogRef: MatDialogRef<ConfirmDeleteDialogComponent>,
    @Inject(MAT_DIALOG_DATA) public data: { remoteName: string }
  ) {}

  onConfirm() {
    this.dialogRef.close(true);
  }

  onCancel() {
    this.dialogRef.close(false);
  }
}
