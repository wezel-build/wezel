import { createContext, useContext } from "react";

// ── Heat color functions ─────────────────────────────────────────────────────

export type HeatFn = (heat: number) => {
  border: string;
  bg: string;
  text: string;
};

export const warmHeat: HeatFn = (heat) => {
  if (heat >= 80) return { border: "#c27458", bg: "#2c1e18", text: "#d4a090" };
  if (heat >= 60) return { border: "#b89860", bg: "#2a2418", text: "#c8b080" };
  if (heat >= 40) return { border: "#90885c", bg: "#24221a", text: "#b0a87c" };
  if (heat >= 20) return { border: "#7c8898", bg: "#1e2228", text: "#94a0b0" };
  return { border: "#3c3830", bg: "#1c1a18", text: "#686058" };
};

const slateHeat: HeatFn = (heat) => {
  if (heat >= 80) return { border: "#b45454", bg: "#2a1a1a", text: "#d4908f" };
  if (heat >= 60) return { border: "#b08448", bg: "#2a2218", text: "#c4a872" };
  if (heat >= 40) return { border: "#8a8444", bg: "#242318", text: "#b0a870" };
  if (heat >= 20) return { border: "#6870a8", bg: "#1c1e2e", text: "#8e94b8" };
  return { border: "#3a4050", bg: "#181c24", text: "#5c6478" };
};

export const lightHeat: HeatFn = (heat) => {
  if (heat >= 80) return { border: "#c0392b", bg: "#fdecea", text: "#922b21" };
  if (heat >= 60) return { border: "#d4870e", bg: "#fef5e7", text: "#9a6508" };
  if (heat >= 40) return { border: "#839034", bg: "#f4f6e8", text: "#5c6624" };
  if (heat >= 20) return { border: "#6875b0", bg: "#eceef6", text: "#4a5488" };
  return { border: "#b0b8c4", bg: "#f0f2f4", text: "#8890a0" };
};

// ── Theme types and constants ────────────────────────────────────────────────

export interface Colors {
  bg: string;
  surface: string;
  surface2: string;
  surface3: string;
  border: string;
  text: string;
  textMid: string;
  textDim: string;
  accent: string;
  green: string;
  amber: string;
  red: string;
  pink: string;
  cyan: string;
}

export interface Theme {
  C: Colors;
  heatColor: HeatFn;
  dark: boolean;
}

// Slimmed context — only what components actually need from React context.
// Color values are accessed statically via CSS variables in lib/colors.ts.
// `colors` is included for renderers (e.g. Vega-Lite) that cannot read CSS variables.
export interface ThemeCtxValue {
  heatColor: HeatFn;
  dark: boolean;
  colors: Colors;
}

const WARM: Theme = {
  heatColor: warmHeat,
  dark: true,
  C: {
    bg: "#141210",
    surface: "#1c1a17",
    surface2: "#242220",
    surface3: "#2e2c28",
    border: "#38342e",
    text: "#d0ccc4",
    textMid: "#9a9488",
    textDim: "#686058",
    accent: "#b08868",
    green: "#7a9870",
    amber: "#b89860",
    red: "#c27458",
    pink: "#a88078",
    cyan: "#7a9ca0",
  },
};

const SLATE: Theme = {
  heatColor: slateHeat,
  dark: true,
  C: {
    bg: "#101218",
    surface: "#171b22",
    surface2: "#1e222c",
    surface3: "#262c38",
    border: "#2c3340",
    text: "#c8ccd4",
    textMid: "#8890a0",
    textDim: "#586070",
    accent: "#7880b0",
    green: "#6a9a78",
    amber: "#b09868",
    red: "#b45454",
    pink: "#a06888",
    cyan: "#6898a0",
  },
};

const LIGHT: Theme = {
  heatColor: lightHeat,
  dark: false,
  C: {
    bg: "#f8f7f5",
    surface: "#ffffff",
    surface2: "#f0eeeb",
    surface3: "#e6e3de",
    border: "#d8d4ce",
    text: "#2c2a28",
    textMid: "#5c5850",
    textDim: "#908880",
    accent: "#8a6e50",
    green: "#4a7a52",
    amber: "#9a7830",
    red: "#b04838",
    pink: "#985868",
    cyan: "#4a7a80",
  },
};

export type ThemeKey = "warm" | "slate" | "light";
export const THEME_ORDER: ThemeKey[] = ["warm", "slate", "light"];
export const THEMES: Record<ThemeKey, Theme> = {
  warm: WARM,
  slate: SLATE,
  light: LIGHT,
};

export const ThemeCtx = createContext<ThemeCtxValue>({
  heatColor: warmHeat,
  dark: true,
  colors: WARM.C,
});
export function useTheme() {
  return useContext(ThemeCtx);
}
