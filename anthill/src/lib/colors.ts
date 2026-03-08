// Static CSS variable references — use these instead of pulling hex values
// from React context. The actual values are set on :root by Shell.tsx when the
// theme changes, and exposed to Tailwind via @theme in index.css.

export const C = {
  bg: "var(--c-bg)",
  surface: "var(--c-surface)",
  surface2: "var(--c-surface2)",
  surface3: "var(--c-surface3)",
  border: "var(--c-border)",
  text: "var(--c-text)",
  textMid: "var(--c-text-mid)",
  textDim: "var(--c-text-dim)",
  accent: "var(--c-accent)",
  green: "var(--c-green)",
  amber: "var(--c-amber)",
  red: "var(--c-red)",
  pink: "var(--c-pink)",
  cyan: "var(--c-cyan)",
} as const;

/**
 * Returns a CSS color-mix() expression that blends a CSS variable (or any
 * color value) with transparency. `percent` is the opacity expressed as a
 * percentage (0–100).
 *
 * Examples:
 *   alpha(C.accent, 13)  →  "color-mix(in srgb, var(--c-accent) 13%, transparent)"
 *   alpha("#c27458", 20) →  "color-mix(in srgb, #c27458 20%, transparent)"
 */
export function alpha(color: string, percent: number): string {
  return `color-mix(in srgb, ${color} ${percent}%, transparent)`;
}
