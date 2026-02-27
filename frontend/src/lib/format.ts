export const MONO = "'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace";
export const SANS = "'Inter', -apple-system, system-ui, sans-serif";

export function fmtValue(value: number, unit?: string): string {
  if (!unit) return value.toLocaleString();
  switch (unit) {
    case "ms":
      if (value >= 60_000) return `${(value / 60_000).toFixed(1)}m`;
      if (value >= 1000) return `${(value / 1000).toFixed(1)}s`;
      return `${value}ms`;
    case "bytes":
      if (value >= 1_048_576) return `${(value / 1_048_576).toFixed(1)} MB`;
      if (value >= 1024) return `${(value / 1024).toFixed(1)} KB`;
      return `${value} B`;
    case "lines":
      if (value >= 1000) return `${(value / 1000).toFixed(1)}k`;
      return `${value}`;
    default:
      return `${value.toLocaleString()} ${unit}`;
  }
}

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
