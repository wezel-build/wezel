export const MONO = "'CommitMono', 'Commit Mono', 'JetBrains Mono', 'Fira Code', monospace";
export const SANS = "'Inter', -apple-system, system-ui, sans-serif";

export function fmtValue(value: number, unit?: string): string {
  if (!unit) return value.toLocaleString();
  switch (unit) {
    case "ms":
      if (value >= 60_000) {
        const m = Math.floor(value / 60_000);
        const s = Math.round((value % 60_000) / 1000);
        return `${m}m ${s}s`;
      }
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

export function fmtMs(ms: number, precise?: boolean): string {
  if (!precise) {
    if (ms >= 60_000) {
      const m = Math.floor(ms / 60_000);
      const s = Math.round((ms % 60_000) / 1000);
      return `${m}m ${s}s`;
    }
    if (ms >= 1000) return `${(ms / 1000).toFixed(1)}s`;
    return `${ms}ms`;
  }
  if (ms >= 3_600_000) {
    const h = Math.floor(ms / 3_600_000);
    const m = Math.floor((ms % 3_600_000) / 60_000);
    const s = ((ms % 60_000) / 1000).toFixed(1);
    return `${h}h ${m}m ${s}s`;
  }
  if (ms >= 60_000) {
    const m = Math.floor(ms / 60_000);
    const s = ((ms % 60_000) / 1000).toFixed(2);
    return `${m}m ${s}s`;
  }
  if (ms >= 1000) return `${(ms / 1000).toFixed(3)}s`;
  return `${ms.toFixed(1)}ms`;
}

export function fmtTime(ts: string): string {
  const d = /^\d+$/.test(ts) ? new Date(Number(ts) * 1000) : new Date(ts);
  const mon = (d.getMonth() + 1).toString().padStart(2, "0");
  const day = d.getDate().toString().padStart(2, "0");
  const h = d.getHours().toString().padStart(2, "0");
  const m = d.getMinutes().toString().padStart(2, "0");
  return `${mon}/${day} ${h}:${m}`;
}
