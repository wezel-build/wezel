import { useTheme } from "../lib/theme";
import { fmtMs } from "../lib/format";
import { Stat } from "./Stat";
import type { Scenario, Run } from "../lib/data";

function percentile(sorted: number[], p: number): number {
  if (sorted.length === 0) return 0;
  const idx = (p / 100) * (sorted.length - 1);
  const lo = Math.floor(idx);
  const hi = Math.ceil(idx);
  if (lo === hi) return sorted[lo];
  return sorted[lo] + (sorted[hi] - sorted[lo]) * (idx - lo);
}

function fmtTimespan(runs: Run[]): string {
  if (runs.length === 0) return "—";
  const times = runs.map((r) => {
    const ts = r.timestamp;
    return /^\d+$/.test(ts) ? Number(ts) * 1000 : new Date(ts).getTime();
  });
  const min = Math.min(...times);
  const max = Math.max(...times);
  const diffMs = max - min;
  if (diffMs < 60_000) return `${(diffMs / 1000).toFixed(0)}s`;
  if (diffMs < 3_600_000) return `${(diffMs / 60_000).toFixed(1)}m`;
  if (diffMs < 86_400_000) return `${(diffMs / 3_600_000).toFixed(1)}h`;
  return `${(diffMs / 86_400_000).toFixed(1)}d`;
}

export function Summary({
  scenario,
  selectedRuns,
  heat,
}: {
  scenario: Scenario;
  selectedRuns: Run[];
  heat: Record<string, number>;
}) {
  const { C, heatColor } = useTheme();
  const crateNames = scenario.graph.map((c) => c.name);
  const hotCrates = crateNames
    .map((n) => ({ name: n, heat: heat[n] ?? 0 }))
    .sort((a, b) => b.heat - a.heat)
    .slice(0, 8);

  const sorted = selectedRuns
    .map((r) => r.buildTimeMs)
    .slice()
    .sort((a, b) => a - b);

  const n = sorted.length;
  const avg = n > 0 ? Math.round(sorted.reduce((s, v) => s + v, 0) / n) : 0;
  const med = n > 0 ? Math.round(percentile(sorted, 50)) : 0;
  const p75 = n > 0 ? Math.round(percentile(sorted, 75)) : 0;
  const p90 = n > 0 ? Math.round(percentile(sorted, 90)) : 0;
  const sum = sorted.reduce((s, v) => s + v, 0);

  const dash = "—";

  return (
    <div className="flex flex-col gap-[12px] text-[11px] min-w-[170px]">
      {/* Metrics */}
      <div className="flex flex-col gap-[8px]">
        <Stat
          label="Avg build"
          value={n > 0 ? fmtMs(avg) : dash}
          color={C.amber}
        />
        <Stat
          label="Median"
          value={n > 0 ? fmtMs(med) : dash}
          color={C.amber}
        />
        <Stat label="p75" value={n > 0 ? fmtMs(p75) : dash} color={C.amber} />
        <Stat label="p90" value={n > 0 ? fmtMs(p90) : dash} color={C.red} />
      </div>

      <div className="h-[1px]" style={{ background: C.border }} />

      <div className="flex flex-col gap-[8px]">
        <Stat
          label="Total time"
          value={n > 0 ? fmtMs(sum, true) : dash}
          color={C.cyan}
        />
        <Stat
          label="Timespan"
          value={fmtTimespan(selectedRuns)}
          color={C.textMid}
        />
        <Stat
          label="Runs selected"
          value={`${selectedRuns.length}/${scenario.runs.length}`}
          color={C.accent}
        />
        <Stat
          label="Crates in graph"
          value={`${scenario.graph.length}`}
          color={C.pink}
        />
      </div>

      {selectedRuns.length > 0 &&
        selectedRuns[0].platform &&
        selectedRuns.every((r) => r.platform === selectedRuns[0].platform) && (
          <>
            <div className="h-[1px]" style={{ background: C.border }} />
            <Stat
              label="Platform"
              value={selectedRuns[0].platform}
              color={C.cyan}
            />
          </>
        )}

      <div className="h-[1px]" style={{ background: C.border }} />

      {/* Hottest crates */}
      <div>
        <div
          className="text-[9px] font-bold uppercase tracking-[0.8px] mb-[6px]"
          style={{ color: C.textDim }}
        >
          Rebuild frequency
        </div>
        <div className="flex flex-col gap-[3px]">
          {hotCrates.map((c) => {
            const col = heatColor(c.heat);
            return (
              <div
                key={c.name}
                className="flex items-center gap-[6px] py-[3px] px-[6px] rounded-[3px]"
                style={{
                  background: col.bg,
                  border: `1px solid ${col.border}33`,
                }}
              >
                <span
                  className="text-[10px] font-mono flex-1 overflow-hidden text-ellipsis whitespace-nowrap"
                  style={{ color: col.text }}
                >
                  {c.name}
                </span>
                <span
                  className="text-[9px] font-mono font-bold min-w-[28px] text-right"
                  style={{ color: col.border }}
                >
                  {c.heat}%
                </span>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
