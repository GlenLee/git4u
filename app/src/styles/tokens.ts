import type { GlobalThemeOverrides } from "naive-ui";

export type ThemeMode = "dark" | "light";

type Palette = {
  primary: string;
  primaryHover: string;
  primaryPressed: string;
  primaryText: string;
  accentDim: string;
  bg: string;
  panel: string;
  sunken: string;
  input: string;
  popover: string;
  hover: string;
  active: string;
  border: string;
  borderStrong: string;
  text: string;
  textMuted: string;
  textFaint: string;
  success: string;
  error: string;
  warning: string;
  info: string;
  purple: string;
  orange: string;
};

const dark: Palette = {
  primary: "#63e2b7",
  primaryHover: "#7fe7c4",
  primaryPressed: "#5acea7",
  primaryText: "#101014",
  accentDim: "rgba(99,226,183,.12)",
  bg: "#101014",
  panel: "#18181c",
  sunken: "#0c0c10",
  input: "#1f1f23",
  popover: "#48484e",
  hover: "rgba(255,255,255,.09)",
  active: "rgba(255,255,255,.13)",
  border: "#2d2d30",
  borderStrong: "#48484e",
  text: "rgba(255,255,255,.92)",
  textMuted: "rgba(255,255,255,.64)",
  textFaint: "rgba(255,255,255,.42)",
  success: "#63e2b7",
  error: "#e88080",
  warning: "#f2c97d",
  info: "#70c0e8",
  purple: "#b79aef",
  orange: "#f0a06a",
};

const light: Palette = {
  primary: "#18a058",
  primaryHover: "#36ad6a",
  primaryPressed: "#0c7a43",
  primaryText: "#ffffff",
  accentDim: "rgba(24,160,88,.1)",
  bg: "#f5f6f8",
  panel: "#ffffff",
  sunken: "#eceef2",
  input: "#ffffff",
  popover: "#ffffff",
  hover: "rgba(0,0,0,.05)",
  active: "rgba(0,0,0,.08)",
  border: "#e2e3e8",
  borderStrong: "#d2d4dc",
  text: "rgba(0,0,0,.9)",
  textMuted: "rgba(0,0,0,.6)",
  textFaint: "rgba(0,0,0,.4)",
  success: "#18a058",
  error: "#d03050",
  warning: "#c98a00",
  info: "#2080f0",
  purple: "#7c5cd6",
  orange: "#d96a20",
};

const palettes: Record<ThemeMode, Palette> = { dark, light };

export function palette(mode: ThemeMode): Palette {
  return palettes[mode];
}

export function applyTheme(mode: ThemeMode): void {
  const p = palette(mode);
  const root = document.documentElement;
  const vars: Record<string, string> = {
    "--primary": p.primary,
    "--primary-hover": p.primaryHover,
    "--primary-pressed": p.primaryPressed,
    "--primary-text": p.primaryText,
    "--accent": p.primary,
    "--accent-dim": p.accentDim,
    "--bg": p.bg,
    "--bg-panel": p.panel,
    "--bg-sunken": p.sunken,
    "--bg-input": p.input,
    "--bg-popover": p.popover,
    "--bg-hover": p.hover,
    "--bg-active": p.active,
    "--border": p.border,
    "--border-strong": p.borderStrong,
    "--text": p.text,
    "--text-muted": p.textMuted,
    "--text-faint": p.textFaint,
    "--success": p.success,
    "--green": p.success,
    "--error": p.error,
    "--red": p.error,
    "--danger": p.error,
    "--warning": p.warning,
    "--yellow": p.warning,
    "--info": p.info,
    "--cyan": p.info,
    "--purple": p.purple,
    "--orange": p.orange,
    "--mono": 'SFMono-Regular, Consolas, "Liberation Mono", Menlo, Courier, monospace',
    "--ui": '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, "Microsoft YaHei", "PingFang SC", sans-serif',
    "--radius": "3px",
    "--shadow-2": mode === "dark" ? "0 4px 14px rgba(0,0,0,.42)" : "0 4px 14px rgba(0,0,0,.12)",
  };
  for (const [key, value] of Object.entries(vars)) {
    root.style.setProperty(key, value);
  }
  root.dataset.theme = mode;
}

export function naiveTheme(mode: ThemeMode): GlobalThemeOverrides {
  const p = palette(mode);
  return {
    common: {
      primaryColor: p.primary,
      primaryColorHover: p.primaryHover,
      primaryColorPressed: p.primaryPressed,
      primaryColorSuppl: p.primaryHover,
      bodyColor: p.bg,
      cardColor: p.panel,
      modalColor: p.panel,
      popoverColor: p.input,
      tableColor: p.panel,
      inputColor: p.input,
      actionColor: p.input,
      borderColor: p.border,
      dividerColor: p.border,
      textColorBase: p.text,
      textColor1: p.text,
      textColor2: p.text,
      textColor3: p.textMuted,
      textColorDisabled: p.textFaint,
      placeholderColor: p.textFaint,
      borderRadius: "3px",
      borderRadiusSmall: "3px",
      fontFamily: "var(--ui)",
      fontSize: "13px",
      heightMedium: "30px",
      heightSmall: "24px",
    },
    Button: {
      textColor: p.text,
      textColorHover: p.primary,
      textColorPressed: p.primaryPressed,
      textColorFocus: p.primary,
      border: `1px solid ${p.borderStrong}`,
      borderHover: `1px solid ${p.primary}`,
      borderPressed: `1px solid ${p.primaryPressed}`,
      borderFocus: `1px solid ${p.primary}`,
      color: "transparent",
      colorHover: "transparent",
      colorPressed: "transparent",
      colorFocus: "transparent",
      colorPrimary: p.primary,
      colorPrimaryHover: p.primaryHover,
      colorPrimaryPressed: p.primaryPressed,
      textColorPrimary: p.primaryText,
      textColorPrimaryHover: p.primaryText,
      textColorPrimaryPressed: p.primaryText,
      borderRadiusMedium: "3px",
      fontWeight: "400",
    },
    Input: {
      color: p.input,
      colorFocus: p.input,
      border: `1px solid ${p.borderStrong}`,
      borderHover: `1px solid ${p.primaryHover}`,
      borderFocus: `1px solid ${p.primary}`,
      boxShadowFocus: `0 0 0 2px ${p.accentDim}`,
      textColor: p.text,
      placeholderColor: p.textFaint,
      caretColor: p.primary,
      borderRadius: "3px",
    },
    Select: {
      peers: {
        InternalSelection: {
          color: p.input,
          textColor: p.text,
          placeholderColor: p.textFaint,
          border: `1px solid ${p.borderStrong}`,
          borderHover: `1px solid ${p.primaryHover}`,
          borderActive: `1px solid ${p.primary}`,
          borderFocus: `1px solid ${p.primary}`,
          boxShadowActive: `0 0 0 2px ${p.accentDim}`,
          boxShadowFocus: `0 0 0 2px ${p.accentDim}`,
        },
      },
    },
    Card: {
      color: p.panel,
      borderColor: p.border,
      borderRadius: "3px",
    },
    Tag: {
      borderRadius: "3px",
    },
    Checkbox: {
      colorChecked: p.primary,
      borderChecked: `1px solid ${p.primary}`,
      borderFocus: `1px solid ${p.primary}`,
      checkMarkColor: p.primaryText,
      textColor: p.textMuted,
    },
    Tooltip: {
      color: p.input,
      textColor: p.text,
    },
    Dialog: {
      color: p.panel,
      textColor: p.text,
      titleTextColor: p.text,
    },
    Message: {
      color: p.panel,
      textColor: p.text,
      boxShadow: "0 4px 14px rgba(0,0,0,.28)",
    },
  };
}
