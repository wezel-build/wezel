import { useState, useCallback, useMemo } from "react";
import { useTheme, lightHeat } from "../lib/theme";
import { MONO } from "../lib/format";
import { computeHeat, type Scenario } from "../lib/data";
import { Badge } from "../components/Badge";
import { HeatLegend } from "../components/HeatLegend";
import { PanelHandle } from "../components/PanelHandle";
import { RunList } from "../components/RunList";
import { Summary } from "../components/Summary";
import { layoutGraph, FitViewGraph } from "../components/Graph";

export function DetailView({ scenario }: { scenario: Scenario }) {
  const { C, heatColor } = useTheme();
  const [threshold, setThreshold] = useState(0);
  const [runsWidth, setRunsWidth] = useState(280);
  const [summaryWidth, setSummaryWidth] = useState(190);
  const [selectedIndices, setSelectedIndices] = useState<Set<number>>(
    () => new Set(scenario.runs.map((_, i) => i)),
  );

  const toggleRun = useCallback((i: number) => {
    setSelectedIndices((prev) => {
      const next = new Set(prev);
      if (next.has(i)) next.delete(i);
      else next.add(i);
      return next;
    });
  }, []);

  const selectAll = useCallback(() => {
    setSelectedIndices(new Set(scenario.runs.map((_, i) => i)));
  }, [scenario.runs]);

  const selectNone = useCallback(() => {
    setSelectedIndices(new Set());
  }, []);

  const selectedRuns = useMemo(
    () => scenario.runs.filter((_, i) => selectedIndices.has(i)),
    [scenario.runs, selectedIndices],
  );

  const crateNames = useMemo(
    () => scenario.graph.map((c) => c.name),
    [scenario.graph],
  );

  const heat = useMemo(
    () => computeHeat(selectedRuns, crateNames),
    [selectedRuns, crateNames],
  );

  const filteredGraph = useMemo(() => {
    if (threshold <= 0) return scenario.graph;
    const kept = new Set(
      scenario.graph
        .filter((c) => (heat[c.name] ?? 0) >= threshold)
        .map((c) => c.name),
    );
    return scenario.graph
      .filter((c) => kept.has(c.name))
      .map((c) => ({ ...c, deps: c.deps.filter((d) => kept.has(d)) }));
  }, [scenario.graph, heat, threshold]);

  const { nodes, edges } = useMemo(
    () => layoutGraph(filteredGraph, heat, heatColor),
    [filteredGraph, heat, heatColor],
  );

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        height: "100%",
        gap: 8,
      }}
    >
      {/* Header row */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          gap: 12,
          flexWrap: "wrap",
          flexShrink: 0,
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <span
            style={{
              fontSize: 13,
              fontWeight: 600,
              color: C.text,
              fontFamily: MONO,
            }}
          >
            {scenario.name}
          </span>
          <Badge
            color={scenario.profile === "dev" ? C.textMid : C.amber}
            bg={scenario.profile === "dev" ? C.surface3 : C.amber + "18"}
          >
            {scenario.profile}
          </Badge>
          {scenario.pinned && (
            <span style={{ fontSize: 10, color: C.accent }}>📌 tracked</span>
          )}
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
          <label
            style={{
              display: "flex",
              alignItems: "center",
              gap: 5,
              background: C.surface2,
              border: `1px solid ${threshold > 0 ? C.accent + "55" : C.border}`,
              borderRadius: 4,
              padding: "3px 8px",
              fontSize: 10,
              fontFamily: MONO,
              color: C.textDim,
              cursor: "text",
              transition: "border-color 0.15s",
            }}
          >
            <span
              style={{
                fontWeight: 600,
                letterSpacing: 0.5,
                textTransform: "uppercase",
                fontSize: 9,
              }}
            >
              threshold
            </span>
            <input
              type="number"
              min={0}
              max={100}
              value={threshold}
              onChange={(e) =>
                setThreshold(
                  Math.max(0, Math.min(100, Number(e.target.value) || 0)),
                )
              }
              style={{
                width: 28,
                background: "transparent",
                border: "none",
                color: threshold > 0 ? C.accent : C.textMid,
                fontSize: 11,
                fontFamily: MONO,
                fontWeight: 600,
                textAlign: "right",
                outline: "none",
                padding: 0,
                MozAppearance: "textfield",
              }}
            />
            <span style={{ color: threshold > 0 ? C.accent : C.textDim }}>
              %
            </span>
          </label>
          <HeatLegend />
        </div>
      </div>

      {/* Body: runs list | graph | summary */}
      <div style={{ flex: 1, display: "flex", gap: 0, minHeight: 0 }}>
        {/* Run list */}
        <div
          style={{
            width: runsWidth,
            flexShrink: 0,
            height: "100%",
            overflow: "hidden",
          }}
        >
          <RunList
            runs={scenario.runs}
            selectedIndices={selectedIndices}
            onToggle={toggleRun}
            onSelectAll={selectAll}
            onSelectNone={selectNone}
          />
        </div>
        <PanelHandle
          onDrag={(d) => setRunsWidth((w) => Math.max(180, w + d))}
        />

        {/* Graph */}
        <div
          style={{
            flex: 1,
            borderRadius: 0,
            overflow: "hidden",
            background: C.bg,
          }}
        >
          <FitViewGraph
            nodes={nodes}
            edges={edges}
            colorMode={heatColor === lightHeat ? "light" : "dark"}
            bg={C.surface2}
            surface={C.surface}
            border={C.border}
          />
        </div>

        <PanelHandle
          onDrag={(d) => setSummaryWidth((w) => Math.max(140, w - d))}
        />
        {/* Summary sidebar */}
        <div
          style={{
            width: summaryWidth,
            overflowY: "auto",
            padding: "8px 10px",
            flexShrink: 0,
          }}
        >
          <Summary
            scenario={scenario}
            selectedRuns={selectedRuns}
            heat={heat}
          />
        </div>
      </div>
    </div>
  );
}