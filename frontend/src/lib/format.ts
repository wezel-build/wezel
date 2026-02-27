export const MONO = "'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace";
export const SANS = "'Inter', -apple-system, system-ui, sans-serif";

export function fmtMs(ms: number): string {
  if (ms >= 60_000) return `${(ms / 60_000).toFixed(1)}m`;
  if (ms >= 1000) return `${(ms / 1000).toFixed(1)}s`;
  return `${ms}ms`;
}

export function fmtTime(ts: string): string {
  const d = new Date(ts);
  const mon = (d.getMonth() + 1).toString().padStart(2, "0");
  const day = d.getDate().toString().padStart(2, "0");
  const h = d.getHours().toString().padStart(2, "0");
  const m = d.getMinutes().toString().padStart(2, "0");
  return `${mon}/${day} ${h}:${m}`;
}