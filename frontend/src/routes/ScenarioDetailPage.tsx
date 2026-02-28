import { useState, useCallback, useMemo, useEffect, useRef } from "react";
import { X } from "lucide-react";
import { useTheme, lightHeat } from "../lib/theme";
import { MONO } from "../lib/format";
import { computeHeat, type Scenario } from "../lib/data";
import { Badge } from "../components/Badge";
import { HeatLegend } from "../components/HeatLegend";
import { PanelHandle } from "../components/PanelHandle";
import { RunList } from "../components/RunList";
import { Summary } from "../components/Summary";
import { layoutGraph, FitViewGraph } from "../components/Graph";
import { useKeyboardNav } from "../lib/useKeyboardNav";

export function DetailView({
  scenario,
  keyboardActive = false,
}: {
  scenario: Scenario;
  keyboardActive?: boolean;
}) {
  const { C, heatColor } = useTheme();
  const [threshold, setThreshold] = useState(0);
  const [runsWidth, setRunsWidth] = useState(280);
  const [summaryWidth, setSummaryWidth] = useState(190);
  const [crateFilter, setCrateFilter] = useState<string | null>(null);

  const [selectedIndices, setSelectedIndices] = useState<Set<number>>(
    () => new Set(scenario.runs.map((_, i) => i)),
  );
  const [hlRunIdx, setHlRunIdx] = useState(-1);
  const hlRunIdxRef = useRef(hlRunIdx);
  hlRunIdxRef.current = hlRunIdx;
  const [markedRunIndices, setMarkedRunIndices] = useState<Set<number>>(
    () => new Set(),
  );
  const prevDisplayedOriginalIndices = useRef<number[]>([]);

  const toggleRun = useCallback((i: number) => {
    setSelectedIndices((prev) => {
      const next = new Set(prev);
      if (next.has(i)) next.delete(i);
      else next.add(i);
      return next;
    });
  }, []);

  // Visible runs after crate filter
  const visibleRunIndices = useMemo(() => {
    if (!crateFilter) return null; // null = show all
    const indices: number[] = [];
    scenario.runs.forEach((r, i) => {
      if (r.dirtyCrates.includes(crateFilter)) indices.push(i);
    });
    return new Set(indices);
  }, [scenario.runs, crateFilter]);

  const displayedRuns = useMemo(() => {
    if (!visibleRunIndices) return scenario.runs;
    return scenario.runs.filter((_, i) => visibleRunIndices.has(i));
  }, [scenario.runs, visibleRunIndices]);

  // Map displayed index → original index for selection tracking
  const displayedOriginalIndices = useMemo(() => {
    if (!visibleRunIndices) return scenario.runs.map((_, i) => i);
    return scenario.runs
      .map((_, i) => i)
      .filter((i) => visibleRunIndices.has(i));
  }, [scenario.runs, visibleRunIndices]);

  const displayedSelectedIndices = useMemo(() => {
    const s = new Set<number>();
    displayedOriginalIndices.forEach((origIdx, dispIdx) => {
      if (selectedIndices.has(origIdx)) s.add(dispIdx);
    });
    return s;
  }, [displayedOriginalIndices, selectedIndices]);

  const handleToggleDisplayed = useCallback(
    (displayIdx: number) => {
      const origIdx = displayedOriginalIndices[displayIdx];
      if (origIdx == null) return;
      toggleRun(origIdx);
    },
    [displayedOriginalIndices, toggleRun],
  );

  const handleSelectAllDisplayed = useCallback(() => {
    setSelectedIndices((prev) => {
      const next = new Set(prev);
      for (const origIdx of displayedOriginalIndices) next.add(origIdx);
      return next;
    });
  }, [displayedOriginalIndices]);

  // Keyboard nav for runs when this panel is active
  const runKeyMap = useMemo(() => {
    if (!keyboardActive) return {};
    const moveDown = (e: KeyboardEvent) => {
      if (e.shiftKey) {
        setMarkedRunIndices((prev) => {
          const next = new Set(prev);
          next.add(hlRunIdxRef.current >= 0 ? hlRunIdxRef.current : 0);
          const target =
            hlRunIdxRef.current >= displayedRuns.length - 1
              ? 0
              : hlRunIdxRef.current + 1;
          next.add(target);
          return next;
        });
      } else {
        setMarkedRunIndices(new Set());
      }
      setHlRunIdx((i) => (i >= displayedRuns.length - 1 ? 0 : i + 1));
    };
    const moveUp = (e: KeyboardEvent) => {
      if (e.shiftKey) {
        setMarkedRunIndices((prev) => {
          const next = new Set(prev);
          next.add(hlRunIdxRef.current >= 0 ? hlRunIdxRef.current : 0);
          const target =
            hlRunIdxRef.current <= 0
              ? displayedRuns.length - 1
              : hlRunIdxRef.current - 1;
          next.add(target);
          return next;
        });
      } else {
        setMarkedRunIndices(new Set());
      }
      setHlRunIdx((i) => (i <= 0 ? displayedRuns.length - 1 : i - 1));
    };
    const toggle = () => {
      const marked = markedRunIndices.size > 0 ? markedRunIndices : null;
      if (marked) {
        for (const idx of marked) {
          if (idx >= 0 && idx < displayedRuns.length)
            handleToggleDisplayed(idx);
        }
        setMarkedRunIndices(new Set());
      } else {
        const i = hlRunIdxRef.current;
        if (i >= 0 && i < displayedRuns.length) handleToggleDisplayed(i);
      }
    };
    return {
      ArrowDown: moveDown,
      j: moveDown,
      ArrowUp: moveUp,
      k: moveUp,
      Enter: toggle,
      " ": toggle,
    } as Record<string, (e: KeyboardEvent) => void>;
  }, [
    keyboardActive,
    displayedRuns.length,
    handleToggleDisplayed,
    markedRunIndices,
  ]);

  useKeyboardNav(runKeyMap);

  // Reset run highlight and marks when keyboard focus leaves
  useEffect(() => {
    if (!keyboardActive) {
      setHlRunIdx(-1);
      setMarkedRunIndices(new Set());
    }
  }, [keyboardActive]);

  // Preserve highlight across crate filter changes
  useEffect(() => {
    const prev = prevDisplayedOriginalIndices.current;
    prevDisplayedOriginalIndices.current = displayedOriginalIndices;
    setHlRunIdx((oldIdx) => {
      if (oldIdx < 0) return -1;
      const origIdx = prev[oldIdx];
      if (origIdx == null) return -1;
      const newIdx = displayedOriginalIndices.indexOf(origIdx);
      return newIdx;
    });
  }, [displayedOriginalIndices]);

  const handleSelectNoneDisplayed = useCallback(() => {
    setSelectedIndices((prev) => {
      const next = new Set(prev);
      for (const origIdx of displayedOriginalIndices) next.delete(origIdx);
      return next;
    });
  }, [displayedOriginalIndices]);

  const selectedRuns = useMemo(
    () =>
      scenario.runs.filter(
        (_, i) =>
          selectedIndices.has(i) &&
          (!visibleRunIndices || visibleRunIndices.has(i)),
      ),
    [scenario.runs, selectedIndices, visibleRunIndices],
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

  const highlightedCrates = useMemo(() => {
    if (hlRunIdx < 0 || hlRunIdx >= displayedRuns.length) return undefined;
    return new Set(displayedRuns[hlRunIdx].dirtyCrates);
  }, [hlRunIdx, displayedRuns]);

  const { nodes, edges } = useMemo(
    () =>
      layoutGraph(filteredGraph, heat, heatColor, highlightedCrates, C.accent),
    [filteredGraph, heat, heatColor, highlightedCrates, C.accent],
  );

  const handleNodeClick = useCallback((crateName: string) => {
    setCrateFilter((prev) => (prev === crateName ? null : crateName));
  }, []);

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
            display: "flex",
            flexDirection: "column",
          }}
        >
          {/* Crate filter pill */}
          {crateFilter && (
            <div
              style={{
                padding: "4px 8px",
                borderBottom: `1px solid ${C.border}`,
                display: "flex",
                alignItems: "center",
                gap: 5,
                flexShrink: 0,
              }}
            >
              <span
                style={{
                  fontSize: 8,
                  color: C.textDim,
                  fontWeight: 700,
                  letterSpacing: 0.5,
                  textTransform: "uppercase",
                }}
              >
                crate
              </span>
              <span
                style={{
                  display: "inline-flex",
                  alignItems: "center",
                  gap: 3,
                  fontSize: 10,
                  fontFamily: MONO,
                  fontWeight: 600,
                  color: C.accent,
                  background: C.accent + "18",
                  border: `1px solid ${C.accent}44`,
                  borderRadius: 3,
                  padding: "1px 4px 1px 6px",
                }}
              >
                {crateFilter}
                <button
                  onClick={() => setCrateFilter(null)}
                  style={{
                    background: "none",
                    border: "none",
                    cursor: "pointer",
                    padding: 0,
                    display: "flex",
                    color: C.accent,
                  }}
                >
                  <X size={9} />
                </button>
              </span>
              <span style={{ fontSize: 9, color: C.textDim, fontFamily: MONO }}>
                {displayedRuns.length}/{scenario.runs.length}
              </span>
            </div>
          )}
          <div style={{ flex: 1, overflow: "hidden" }}>
            <RunList
              runs={displayedRuns}
              selectedIndices={displayedSelectedIndices}
              onToggle={handleToggleDisplayed}
              onSelectAll={handleSelectAllDisplayed}
              onSelectNone={handleSelectNoneDisplayed}
              hlIdx={hlRunIdx}
              markedIndices={markedRunIndices}
            />
          </div>
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
            onNodeClick={handleNodeClick}
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
