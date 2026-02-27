import { useTheme } from "../lib/theme";
import { MONO } from "../lib/format";
import { fmtMs } from "../lib/format";
import { Stat } from "./Stat";
import type { Scenario, Run } from "../lib/data";

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

  const avgBuild =
    selectedRuns.length > 0
      ? Math.round(
          selectedRuns.reduce((s, r) => s + r.buildTimeMs, 0) /
            selectedRuns.length,
        )
      : 0;

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 12,
        fontSize: 11,
        minWidth: 170,
      }}
    >
      {/* Metrics */}
      <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
        <Stat
          label="Avg build"
          value={selectedRuns.length > 0 ? fmtMs(avgBuild) : "—"}
          color={C.amber}
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

      <div style={{ height: 1, background: C.border }} />

      {/* Hottest crates */}
      <div>
        <div
          style={{
            fontSize: 9,
            fontWeight: 700,
            color: C.textDim,
            letterSpacing: 0.8,
            textTransform: "uppercase",
            marginBottom: 6,
          }}
        >
          Rebuild frequency
        </div>
        <div style={{ display: "flex", flexDirection: "column", gap: 3 }}>
          {hotCrates.map((c) => {
            const col = heatColor(c.heat);
            return (
              <div
                key={c.name}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 6,
                  padding: "3px 6px",
                  borderRadius: 3,
                  background: col.bg,
                  border: `1px solid ${col.border}33`,
                }}
              >
                <span
                  style={{
                    fontSize: 10,
                    fontFamily: MONO,
                    color: col.text,
                    flex: 1,
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    whiteSpace: "nowrap",
                  }}
                >
                  {c.name}
                </span>
                <span
                  style={{
                    fontSize: 9,
                    fontFamily: MONO,
                    color: col.border,
                    fontWeight: 700,
                    minWidth: 28,
                    textAlign: "right",
                  }}
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
