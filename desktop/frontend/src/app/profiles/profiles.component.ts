import { CommonModule } from "@angular/common";
import {
  ChangeDetectionStrategy,
  ChangeDetectorRef,
  Component,
  OnDestroy,
  OnInit,
} from "@angular/core";
import { AppService } from "../app.service";
import { BehaviorSubject, Subscription } from "rxjs";
import { FormsModule } from "@angular/forms";

// Material Design imports
import { MatCardModule } from "@angular/material/card";
import { MatButtonModule } from "@angular/material/button";
import { MatIconModule } from "@angular/material/icon";
import { MatFormFieldModule } from "@angular/material/form-field";
import { MatInputModule } from "@angular/material/input";
import { MatSelectModule } from "@angular/material/select";
import { MatExpansionModule } from "@angular/material/expansion";
import { MatChipsModule } from "@angular/material/chips";
import { MatTooltipModule } from "@angular/material/tooltip";
import { MatSnackBarModule } from "@angular/material/snack-bar";
import { MatDividerModule } from "@angular/material/divider";

@Component({
  selector: "app-profiles",
  imports: [
    CommonModule,
    FormsModule,
    MatCardModule,
    MatButtonModule,
    MatIconModule,
    MatFormFieldModule,
    MatInputModule,
    MatSelectModule,
    MatExpansionModule,
    MatChipsModule,
    MatTooltipModule,
    MatSnackBarModule,
    MatDividerModule,
  ],
  templateUrl: "./profiles.component.html",
  styleUrl: "./profiles.component.scss",
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ProfilesComponent implements OnInit, OnDestroy {
  Date = Date;
  private changeDetectorSub: Subscription | undefined;

  saveBtnText$ = new BehaviorSubject<string>("Save ✓");

  constructor(
    public readonly appService: AppService,
    private readonly cdr: ChangeDetectorRef
  ) {}

  ngOnInit(): void {
    this.changeDetectorSub = this.appService.configInfo$.subscribe(() =>
      this.cdr.detectChanges()
    );
    this.appService.getConfigInfo();
  }

  ngOnDestroy(): void {}

  addProfile() {
    this.appService.addProfile();
  }

  removeProfile(idx: number) {
    this.appService.removeProfile(idx);
  }

  saveConfigInfo() {
    this.appService.saveConfigInfo();
    this.saveBtnText$.next("Saved ~");
    setTimeout(() => this.saveBtnText$.next("Save ✓"), 1000);
    this.cdr.detectChanges();
  }

  addIncludePath(profileIndex: number) {
    this.appService.addIncludePath(profileIndex);
    this.cdr.detectChanges();
  }

  removeIncludePath(profileIndex: number, idx: number) {
    this.appService.removeIncludePath(profileIndex, idx);
    this.cdr.detectChanges();
  }

  addExcludePath(profileIndex: number) {
    this.appService.addExcludePath(profileIndex);
    this.cdr.detectChanges();
  }

  removeExcludePath(profileIndex: number, idx: number) {
    this.appService.removeExcludePath(profileIndex, idx);
    this.cdr.detectChanges();
  }

  trackByFn(index: number, _item: any): number {
    return index;
  }

  // Helper method to generate number ranges for selectors
  getNumberRange(start: number, end: number): number[] {
    return Array.from({ length: end - start + 1 }, (_, i) => start + i);
  }

  // Get profile description for expansion panel
  getProfileDescription(setting: any): string {
    const from = setting.from || "Not configured";
    const to = setting.to || "Not configured";
    return `${from} → ${to}`;
  }

  // From path helpers
  getFromRemote(setting: any): string {
    if (!setting.from) return "";
    const colonIndex = setting.from.indexOf(":");
    return colonIndex > 0 ? setting.from.substring(0, colonIndex) : "";
  }

  getFromPath(setting: any): string {
    if (!setting.from) return "";
    const colonIndex = setting.from.indexOf(":");
    return colonIndex > 0
      ? setting.from.substring(colonIndex + 1)
      : setting.from;
  }

  updateFromPath(setting: any, remote: string, path: string): void {
    if (remote) {
      setting.from = `${remote}:${path || ""}`;
    } else {
      setting.from = path || "";
    }
    this.cdr.detectChanges();
  }

  // To path helpers
  getToRemote(setting: any): string {
    if (!setting.to) return "";
    const colonIndex = setting.to.indexOf(":");
    return colonIndex > 0 ? setting.to.substring(0, colonIndex) : "";
  }

  getToPath(setting: any): string {
    if (!setting.to) return "";
    const colonIndex = setting.to.indexOf(":");
    return colonIndex > 0 ? setting.to.substring(colonIndex + 1) : setting.to;
  }

  updateToPath(setting: any, remote: string, path: string): void {
    if (remote) {
      setting.to = `${remote}:${path || ""}`;
    } else {
      setting.to = path || "";
    }
    this.cdr.detectChanges();
  }

  // Include path helpers
  getIncludePathType(setting: any, index: number): string {
    const path = setting.included_paths[index];
    if (!path) return "folder";
    return path.endsWith("/**") ? "folder" : "file";
  }

  getIncludePathValue(setting: any, index: number): string {
    const path = setting.included_paths[index];
    if (!path) return "";
    return path.endsWith("/**") ? path.slice(0, -3) : path;
  }

  updateIncludePathType(setting: any, index: number, type: string): void {
    const currentValue = this.getIncludePathValue(setting, index);
    if (type === "folder") {
      setting.included_paths[index] = currentValue + "/**";
    } else {
      setting.included_paths[index] = currentValue;
    }
    this.cdr.detectChanges();
  }

  updateIncludePathValue(setting: any, index: number, value: string): void {
    const currentType = this.getIncludePathType(setting, index);
    if (currentType === "folder") {
      setting.included_paths[index] = value + "/**";
    } else {
      setting.included_paths[index] = value;
    }
    this.cdr.detectChanges();
  }

  // Exclude path helpers
  getExcludePathType(setting: any, index: number): string {
    const path = setting.excluded_paths[index];
    if (!path) return "folder";
    return path.endsWith("/**") ? "folder" : "file";
  }

  getExcludePathValue(setting: any, index: number): string {
    const path = setting.excluded_paths[index];
    if (!path) return "";
    return path.endsWith("/**") ? path.slice(0, -3) : path;
  }

  updateExcludePathType(setting: any, index: number, type: string): void {
    const currentValue = this.getExcludePathValue(setting, index);
    if (type === "folder") {
      setting.excluded_paths[index] = currentValue + "/**";
    } else {
      setting.excluded_paths[index] = currentValue;
    }
    this.cdr.detectChanges();
  }

  updateExcludePathValue(setting: any, index: number, value: string): void {
    const currentType = this.getExcludePathType(setting, index);
    if (currentType === "folder") {
      setting.excluded_paths[index] = value + "/**";
    } else {
      setting.excluded_paths[index] = value;
    }
    this.cdr.detectChanges();
  }
}
