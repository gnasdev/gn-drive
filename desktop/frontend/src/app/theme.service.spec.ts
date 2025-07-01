import { TestBed } from '@angular/core/testing';
import { ThemeService } from './theme.service';

describe('ThemeService', () => {
  let service: ThemeService;

  beforeEach(() => {
    TestBed.configureTestingModule({});
    service = TestBed.inject(ThemeService);
    
    // Clear localStorage before each test
    localStorage.clear();
    
    // Remove any existing theme classes
    document.body.classList.remove('dark-theme', 'light-theme');
    document.documentElement.classList.remove('dark-theme', 'light-theme');
  });

  it('should be created', () => {
    expect(service).toBeTruthy();
  });

  it('should initialize with system preference when no saved theme', () => {
    // Mock system preference for dark mode
    Object.defineProperty(window, 'matchMedia', {
      writable: true,
      value: jest.fn().mockImplementation(query => ({
        matches: query === '(prefers-color-scheme: dark)',
        media: query,
        onchange: null,
        addListener: jest.fn(),
        removeListener: jest.fn(),
        addEventListener: jest.fn(),
        removeEventListener: jest.fn(),
        dispatchEvent: jest.fn(),
      })),
    });

    const newService = new ThemeService();
    expect(newService.isDarkMode).toBe(true);
  });

  it('should toggle dark mode', () => {
    const initialMode = service.isDarkMode;
    service.toggleDarkMode();
    expect(service.isDarkMode).toBe(!initialMode);
  });

  it('should save theme preference to localStorage', () => {
    service.setDarkMode(true);
    expect(localStorage.getItem('dark-mode')).toBe('true');
    
    service.setDarkMode(false);
    expect(localStorage.getItem('dark-mode')).toBe('false');
  });

  it('should apply dark theme classes to DOM', () => {
    service.setDarkMode(true);
    
    setTimeout(() => {
      expect(document.body.classList.contains('dark-theme')).toBe(true);
      expect(document.documentElement.classList.contains('dark-theme')).toBe(true);
    }, 10);
  });

  it('should apply light theme classes to DOM', () => {
    service.setDarkMode(false);
    
    setTimeout(() => {
      expect(document.body.classList.contains('light-theme')).toBe(true);
      expect(document.documentElement.classList.contains('light-theme')).toBe(true);
    }, 10);
  });

  it('should emit theme changes', (done) => {
    service.isDarkMode$.subscribe(isDark => {
      if (isDark) {
        expect(isDark).toBe(true);
        done();
      }
    });
    
    service.setDarkMode(true);
  });
});
