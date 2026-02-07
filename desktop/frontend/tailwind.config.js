/** @type {import('tailwindcss').Config} */
module.exports = {
  content: [
    "./src/**/*.{html,ts}",
    "./node_modules/primeng/**/*.{js,mjs}",
  ],
  darkMode: 'class',
  theme: {
    screens: {
      'sm': '640px',
      'md': '768px',
      'lg': '960px',
      'xl': '1280px',
      '2xl': '1536px',
    },
    extend: {
      colors: {
        // System colors using CSS variables
        sys: {
          // Background
          'bg': 'var(--color-bg-primary)',
          'bg-secondary': 'var(--color-bg-secondary)',
          'bg-tertiary': 'var(--color-bg-tertiary)',
          'bg-inverse': 'var(--color-bg-inverse)',
          // Foreground
          'fg': 'var(--color-fg-primary)',
          'fg-secondary': 'var(--color-fg-secondary)',
          'fg-tertiary': 'var(--color-fg-tertiary)',
          'fg-inverse': 'var(--color-fg-inverse)',
          'fg-muted': 'var(--color-fg-muted)',
          // Border
          'border': 'var(--color-border-default)',
          'border-subtle': 'var(--color-border-subtle)',
          'border-muted': 'var(--color-border-muted)',
          // Accent
          'accent': 'var(--color-accent-primary)',
          'accent-secondary': 'var(--color-accent-secondary)',
          'accent-success': 'var(--color-accent-success)',
          'accent-warning': 'var(--color-accent-warning)',
          'accent-danger': 'var(--color-accent-danger)',
          'accent-info': 'var(--color-accent-info)',
          // Status
          'status-success': 'var(--color-status-success)',
          'status-success-bg': 'var(--color-status-success-bg)',
          'status-warning': 'var(--color-status-warning)',
          'status-warning-bg': 'var(--color-status-warning-bg)',
          'status-error': 'var(--color-status-error)',
          'status-error-bg': 'var(--color-status-error-bg)',
          'status-info': 'var(--color-status-info)',
          'status-info-bg': 'var(--color-status-info-bg)',
        },
        // Keep existing colors for backward compatibility
        primary: {
          50: '#eff6ff',
          100: '#dbeafe',
          200: '#bfdbfe',
          300: '#93c5fd',
          400: '#60a5fa',
          500: '#3b82f6',
          600: '#2563eb',
          700: '#1d4ed8',
          800: '#1e40af',
          900: '#1e3a8a',
        },
        gray: {
          50: '#f9fafb',
          100: '#f3f4f6',
          200: '#e5e7eb',
          300: '#d1d5db',
          400: '#9ca3af',
          500: '#6b7280',
          600: '#4b5563',
          700: '#374151',
          800: '#1f2937',
          900: '#111827',
        },
      },
      boxShadow: {
        'neo': '4px 4px 0px 0px var(--color-shadow)',
        'neo-sm': '2px 2px 0px 0px var(--color-shadow)',
        'neo-lg': '6px 6px 0px 0px var(--color-shadow)',
      },
      fontFamily: {
        sans: ['Roboto', 'sans-serif'],
      },
      spacing: {
        '18': '4.5rem',
        '88': '22rem',
      },
      animation: {
        'fade-in': 'fadeIn 0.3s ease-in-out',
        'slide-up': 'slideUp 0.3s ease-out',
        'overlay-in': 'overlayIn 150ms ease-out',
        'overlay-out': 'overlayOut 150ms ease-in',
      },
      keyframes: {
        fadeIn: {
          '0%': { opacity: '0' },
          '100%': { opacity: '1' },
        },
        slideUp: {
          '0%': { transform: 'translateY(10px)', opacity: '0' },
          '100%': { transform: 'translateY(0)', opacity: '1' },
        },
        overlayIn: {
          '0%': { opacity: '0', transform: 'scaleY(0.8)' },
          '100%': { opacity: '1', transform: 'scaleY(1)' },
        },
        overlayOut: {
          '0%': { opacity: '1', transform: 'scaleY(1)' },
          '100%': { opacity: '0', transform: 'scaleY(0.8)' },
        },
      },
    },
  },
  plugins: [],
}
