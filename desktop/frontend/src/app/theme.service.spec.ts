import { TestBed } from "@angular/core/testing";
import { ThemeService } from "./theme.service";

describe("ThemeService", () => {
  let service: ThemeService;

  beforeEach(() => {
    TestBed.configureTestingModule({});
    service = TestBed.inject(ThemeService);

    // Remove any existing theme classes
    document.body.classList.remove("dark-theme", "light-theme");
    document.documentElement.classList.remove("dark-theme", "light-theme");
  });

  it("should be created", () => {
    expect(service).toBeTruthy();
  });

  it("should always be in dark mode", () => {
    expect(service.isDarkMode).toBe(true);
  });

  it("should apply dark theme classes to DOM on initialization", () => {
    setTimeout(() => {
      expect(document.body.classList.contains("dark-theme")).toBe(true);
      expect(document.documentElement.classList.contains("dark-theme")).toBe(
        true
      );
    }, 10);
  });

  it("should always emit dark mode as true", (done) => {
    service.isDarkMode$.subscribe((isDark) => {
      expect(isDark).toBe(true);
      done();
    });
  });
});
