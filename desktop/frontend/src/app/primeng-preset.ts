import { definePreset } from "@primeuix/themes";
import Aura from "@primeuix/themes/aura";

const NsDrivePreset = definePreset(Aura, {
  semantic: {
    primary: {
      50: "{blue.50}",
      100: "{blue.100}",
      200: "{blue.200}",
      300: "{blue.300}",
      400: "{blue.400}",
      500: "{blue.500}",
      600: "{blue.600}",
      700: "{blue.700}",
      800: "{blue.800}",
      900: "{blue.900}",
      950: "{blue.950}",
    },
    colorScheme: {
      dark: {
        surface: {
          0: "#ffffff",
          50: "#f9fafb",
          100: "#f3f4f6",
          200: "#e5e7eb",
          300: "#d1d5db",
          400: "#9ca3af",
          500: "#6b7280",
          600: "#4b5563",
          700: "#374151",
          800: "#1f2937",
          900: "#111827",
          950: "#030712",
        },
        primary: {
          color: "{blue.500}",
          contrastColor: "#ffffff",
          hoverColor: "{blue.400}",
          activeColor: "{blue.300}",
        },
        highlight: {
          background: "color-mix(in srgb, {blue.500}, transparent 84%)",
          focusBackground: "color-mix(in srgb, {blue.500}, transparent 76%)",
          color: "rgba(255,255,255,.87)",
          focusColor: "rgba(255,255,255,.87)",
        },
      },
    },
  },
  components: {
    button: {
      root: {
        borderRadius: "0.5rem",
      },
    },
    select: {
      root: {
        borderRadius: "0.5rem",
      },
    },
    inputtext: {
      root: {
        borderRadius: "0.5rem",
      },
    },
    dialog: {
      root: {
        borderRadius: "0.75rem",
      },
    },
    toast: {
      root: {
        borderRadius: "0.75rem",
      },
    },
    card: {
      root: {
        background: "{surface.800}",
      },
      colorScheme: {
        dark: {
          root: {
            background: "{surface.800}",
          },
        },
      },
    },
    toolbar: {
      root: {
        padding: "0.75rem 1.25rem",
      },
      colorScheme: {
        dark: {
          root: {
            background: "{surface.800}",
          },
        },
      },
    },
    tabs: {
      colorScheme: {
        dark: {
          tablist: {
            background: "transparent",
          },
          tab: {
            background: "transparent",
            hoverBackground: "{surface.800}",
            activeBackground: "transparent",
            color: "{surface.400}",
            hoverColor: "{surface.100}",
            activeColor: "{primary.400}",
          },
        },
      },
    },
    datatable: {
      colorScheme: {
        dark: {
          header: {
            background: "{surface.800}",
          },
          row: {
            background: "{surface.800}",
            hoverBackground: "{surface.700}",
          },
        },
      },
    },
    progressbar: {
      root: {
        height: "0.5rem",
      },
      colorScheme: {
        dark: {
          root: {
            background: "{surface.700}",
          },
          value: {
            background: "{primary.500}",
          },
        },
      },
    },
    tag: {
      root: {
        fontSize: "0.75rem",
        padding: "0.25rem 0.5rem",
      },
    },
    selectbutton: {
      root: {
        borderRadius: "0.5rem",
      },
    },
  },
} as any);

export default NsDrivePreset;
