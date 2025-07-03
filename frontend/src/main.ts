import { bootstrapApplication } from "@angular/platform-browser";
import { appConfig } from "./app/app.config";
import { AppComponent } from "./app/app.component";

// Import Wails runtime to ensure it's loaded before the application starts
import "@wailsio/runtime";

bootstrapApplication(AppComponent, appConfig).catch((err) =>
  console.error(err)
);
