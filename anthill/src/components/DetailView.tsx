import { useState, useCallback, useMemo, useEffect, useRef } from "react";
import { X } from "lucide-react";
import { useTheme } from "../lib/theme";
import { computeHeat } from "../lib/data";
import { useScenario } from "../lib/hooks";
import { Badge } from "./Badge";
import { HeatLegend } from "./HeatLegend";
import { PanelHandle } from "./PanelHandle";
import { RunList } from "./RunList";
import { Summary } from "./Summary";
import { BuildTimingsChart } from "./BuildTimingsChart";
import { useKeyboardNav } from "../lib/useKeyboardNav";

const EMPTY_RUNS: {
  user: string;
  platform: string;
  timestamp: string;
  commit: string;
  buildTimeMs: number;
  dirtyCrates: string[];
}[] = [];
const EMPTY_GRAPH: { name: string; deps: string[] }[] = [];

function runKey(r: { timestamp: string; commit: string; user: string }) {
  return `${r.timestamp}|${r.commit}|${r.user}`;
}

// TODO: Consider grouping the ~15 useState calls below into one or two reducer objects
// (e.g. panelState, selectionState) to reduce the number of individual state variables.

export function DetailView({
  scenarioId,
  keyboardActive = false,
  userFilter = [],
}: {
  scenarioId: number;
  keyboardActive?: boolean;
  userFilter?: string[];
}) {
  const { scenario: rawScenario, loading, error } = useScenario(scenarioId);

  const scenario = useMemo(() => {
    if (!rawScenario) return null;
    if (userFilter.length === 0) return rawScenario;
    return {
      ...rawScenario,
      runs: rawScenario.runs.filter((r) => userFilter.includes(r.user)),
    };
  }, [rawScenario, userFilter]);

  const runs = (scenario?.runs ?? EMPTY_RUNS)
    .slice()
    .sort((a, b) => b.timestamp.localeCompare(a.timestamp));
  const graph = scenario?.graph ?? EMPTY_GRAPH;

  const { C, heatColor } = useTheme();
  const [threshold, setThreshold] = useState(0);
  const [runsWidth, setRunsWidth] = useState(280);
  const [summaryWidth, setSummaryWidth] = useState(190);
  const [crateFilter, setCrateFilter] = useState<string | null>(null);
  const [focusedCrate, setFocusedCrate] = useState<string | null>(null);

  const [selectedKeys, setSelectedKeys] = useState<Set<string>>(
    () => new Set(),
  );

  // Select all runs when scenario changes or on first load (render-time adjustment)
  const [prevScenarioId, setPrevScenarioId] = useState(scenarioId);
  if (scenarioId !== prevScenarioId) {
    setPrevScenarioId(scenarioId);
    if (runs.length > 0) {
      setSelectedKeys(new Set(runs.map(runKey)));
    }
  } else if (selectedKeys.size === 0 && runs.length > 0) {
    setSelectedKeys(new Set(runs.map(runKey)));
  }

  const [hlRunIdx, setHlRunIdx] = useState(-1);
  const hlRunIdxRef = useRef(hlRunIdx);

  // Sync ref to state inside an effect (not during render) to satisfy React/ESLint rules
  useEffect(() => {
    hlRunIdxRef.current = hlRunIdx;
  }, [hlRunIdx]);

  const [markedRunIndices, setMarkedRunIndices] = useState<Set<number>>(
    () => new Set(),
  );
  const prevDisplayedOriginalIndices = useRef<number[]>([]);

  const toggleRun = useCallback(
    (i: number) => {
      const key = runKey(runs[i]);
      setSelectedKeys((prev) => {
        const next = new Set(prev);
        if (next.has(key)) next.delete(key);
        else next.add(key);
        return next;
      });
    },
    [runs],
  );

  // Visible runs after crate filter
  const visibleRunIndices = useMemo(() => {
    if (!crateFilter) return null; // null = show all
    const indices: number[] = [];
    runs.forEach((r, i) => {
      if (r.dirtyCrates.includes(crateFilter)) indices.push(i);
    });
    return new Set(indices);
  }, [runs, crateFilter]);

  const displayedRuns = useMemo(() => {
    if (!visibleRunIndices) return runs;
    return runs.filter((_, i) => visibleRunIndices.has(i));
  }, [runs, visibleRunIndices]);

  // Map displayed index → original index for selection tracking
  const displayedOriginalIndices = useMemo(() => {
    if (!visibleRunIndices) return runs.map((_, i) => i);
    return runs.map((_, i) => i).filter((i) => visibleRunIndices.has(i));
  }, [runs, visibleRunIndices]);

  const displayedSelectedIndices = useMemo(() => {
    const s = new Set<number>();
    displayedOriginalIndices.forEach((origIdx, dispIdx) => {
      if (selectedKeys.has(runKey(runs[origIdx]))) s.add(dispIdx);
    });
    return s;
  }, [displayedOriginalIndices, selectedKeys, runs]);

  const handleToggleDisplayed = useCallback(
    (displayIdx: number) => {
      const origIdx = displayedOriginalIndices[displayIdx];
      if (origIdx == null) return;
      toggleRun(origIdx);
    },
    [displayedOriginalIndices, toggleRun],
  );

  const handleSelectAllDisplayed = useCallback(() => {
    setSelectedKeys((prev) => {
      const next = new Set(prev);
      for (const origIdx of displayedOriginalIndices)
        next.add(runKey(runs[origIdx]));
      return next;
    });
  }, [displayedOriginalIndices, runs]);

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

  // Capture-phase Escape: fires before the parent's bubble-phase listener.
  // If there's a focused crate, consume the event and just clear focus.
  useEffect(() => {
    if (!focusedCrate) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.stopImmediatePropagation();
      e.preventDefault();
      setFocusedCrate(null);
    };
    window.addEventListener("keydown", handler, true);
    return () => window.removeEventListener("keydown", handler, true);
  }, [focusedCrate]);

  // Reset run highlight and marks when keyboard focus leaves (render-time adjustment)
  const [prevKeyboardActive, setPrevKeyboardActive] = useState(keyboardActive);
  if (keyboardActive !== prevKeyboardActive) {
    setPrevKeyboardActive(keyboardActive);
    if (!keyboardActive) {
      setHlRunIdx(-1);
      setMarkedRunIndices(new Set());
    }
  }

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
    setSelectedKeys((prev) => {
      const next = new Set(prev);
      for (const origIdx of displayedOriginalIndices)
        next.delete(runKey(runs[origIdx]));
      return next;
    });
  }, [displayedOriginalIndices, runs]);

  const selectedRuns = useMemo(
    () =>
      runs.filter(
        (r, i) =>
          selectedKeys.has(runKey(r)) &&
          (!visibleRunIndices || visibleRunIndices.has(i)),
      ),
    [runs, selectedKeys, visibleRunIndices],
  );

  const crateNames = useMemo(() => graph.map((c) => c.name), [graph]);

  const heat = useMemo(
    () => computeHeat(selectedRuns, crateNames),
    [selectedRuns, crateNames],
  );

  const filteredGraph = useMemo(() => {
    if (threshold <= 0) return graph;
    const kept = new Set(
      graph.filter((c) => (heat[c.name] ?? 0) >= threshold).map((c) => c.name),
    );
    return graph
      .filter((c) => kept.has(c.name))
      .map((c) => ({ ...c, deps: c.deps.filter((d) => kept.has(d)) }));
  }, [graph, heat, threshold]);

  const highlightedCrates = useMemo(() => {
    if (hlRunIdx < 0 || hlRunIdx >= displayedRuns.length) return undefined;
    return new Set(displayedRuns[hlRunIdx].dirtyCrates);
  }, [hlRunIdx, displayedRuns]);

  const handleNodeClick = useCallback((crateName: string) => {
    setCrateFilter((prev) => (prev === crateName ? null : crateName));
  }, []);

  const handleNodeFocus = useCallback((crateName: string | null) => {
    setFocusedCrate((prev) => (prev === crateName ? null : crateName));
  }, []);

  // ── Loading / error guards (all hooks are above) ──────────────────────────

  if (error) {
    return (
      <div
        className="flex items-center justify-center h-full text-[11px] font-mono gap-[6px]"
        style={{ color: C.red ?? "#e55" }}
      >
        <span className="font-bold">error:</span> {error}
      </div>
    );
  }

  if (loading || !scenario) {
    return (
      <div className="flex items-center justify-center h-full text-dim text-[11px] font-mono">
        loading…
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full gap-[8px]">
      {/* Header row */}
      <div className="flex items-center justify-between gap-[12px] flex-wrap shrink-0">
        <div className="flex items-center gap-[8px]">
          <span className="text-[13px] font-semibold text-fg font-mono">
            {scenario.name}
          </span>
          <Badge
            color={scenario.profile === "dev" ? C.textMid : C.amber}
            bg={scenario.profile === "dev" ? C.surface3 : C.amber + "18"}
          >
            {scenario.profile}
          </Badge>
          {scenario.platform && (
            <Badge color={C.cyan} bg={C.cyan + "18"}>
              {scenario.platform}
            </Badge>
          )}
          {scenario.pinned && (
            <span className="text-[10px] text-accent">📌 tracked</span>
          )}
        </div>
        <div className="flex items-center gap-[12px]">
          <label
            className="flex items-center gap-[5px] bg-surface2 rounded py-[3px] px-2 text-[10px] font-mono text-dim cursor-text transition-colors duration-150"
            style={{
              border: `1px solid ${threshold > 0 ? C.accent + "55" : C.border}`,
            }}
          >
            <span className="font-semibold tracking-[0.5px] uppercase text-[9px]">
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
              className="w-[28px] bg-transparent border-none text-[11px] font-mono font-semibold text-right outline-none p-0"
              style={{
                color: threshold > 0 ? C.accent : C.textMid,
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
      <div className="flex flex-1 min-h-0">
        {/* Run list */}
        <div
          className="shrink-0 h-full overflow-hidden flex flex-col"
          style={{ width: runsWidth }}
        >
          {/* Crate filter pill */}
          {crateFilter && (
            <div className="py-1 px-2 border-b border-[var(--c-border)] flex items-center gap-[5px] shrink-0">
              <span className="text-[8px] text-dim font-bold tracking-[0.5px] uppercase">
                crate
              </span>
              <span
                className="inline-flex items-center gap-[3px] text-[10px] font-mono font-semibold text-accent rounded-[3px]"
                style={{
                  background: C.accent + "18",
                  border: `1px solid ${C.accent}44`,
                  padding: "1px 4px 1px 6px",
                }}
              >
                {crateFilter}
                <button
                  onClick={() => setCrateFilter(null)}
                  className="bg-transparent border-none cursor-pointer p-0 flex text-accent"
                >
                  <X size={9} />
                </button>
              </span>
              <span className="text-[9px] text-dim font-mono">
                {displayedRuns.length}/{scenario.runs.length}
              </span>
            </div>
          )}
          <div className="flex-1 overflow-hidden">
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
        <div className="flex-1 overflow-hidden bg-bg">
          <BuildTimingsChart
            topo={filteredGraph}
            heat={heat}
            heatColor={heatColor}
            highlightedCrates={highlightedCrates}
            focusedCrate={focusedCrate}
            onNodeClick={handleNodeClick}
            onNodeFocus={handleNodeFocus}
            bg={C.surface2}
            border={C.border}
            accentColor={C.accent}
          />
        </div>

        <PanelHandle
          onDrag={(d) => setSummaryWidth((w) => Math.max(140, w - d))}
        />
        {/* Summary sidebar */}
        <div
          className="overflow-y-auto py-2 px-[10px] shrink-0"
          style={{ width: summaryWidth }}
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
