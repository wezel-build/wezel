import { useState, useCallback, useMemo, useEffect, useRef } from "react";
import { X, Rocket } from "lucide-react";
import { useTheme } from "../lib/theme";
import { C, alpha } from "../lib/colors";
import { computeHeat } from "../lib/data";
import { useObservation } from "../lib/hooks";
import { Badge } from "./Badge";
import { HeatLegend } from "./HeatLegend";
import { PanelHandle } from "./PanelHandle";
import { RunList } from "./RunList";
import { Summary } from "./Summary";
import { BuildTimingsChart } from "./BuildTimingsChart";
import { BenchmarkCreatorModal } from "./BenchmarkCreatorModal";
import { useKeyboardNav } from "../lib/useKeyboardNav";
import { useProject } from "../lib/useProject";

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
  observationId,
  keyboardActive = false,
  userFilter = [],
}: {
  observationId: number;
  keyboardActive?: boolean;
  userFilter?: string[];
}) {
  const { current: currentProject } = useProject();
  const {
    observation: rawObservation,
    loading,
    error,
  } = useObservation(observationId);
  const [benchmarkModal, setBenchmarkModal] = useState<{
    open: boolean;
    initialCrate?: string;
  }>({
    open: false,
  });

  const observation = useMemo(() => {
    if (!rawObservation) return null;
    if (userFilter.length === 0) return rawObservation;
    return {
      ...rawObservation,
      runs: rawObservation.runs.filter((r) => userFilter.includes(r.user)),
    };
  }, [rawObservation, userFilter]);

  const runs = (observation?.runs ?? EMPTY_RUNS)
    .slice()
    .sort((a, b) => b.timestamp.localeCompare(a.timestamp));
  const graph = observation?.graph ?? EMPTY_GRAPH;

  const { heatColor } = useTheme();
  const [threshold, setThreshold] = useState(0);
  const [runsWidth, setRunsWidth] = useState(280);
  const [summaryWidth, setSummaryWidth] = useState(190);
  const [crateFilter, setCrateFilter] = useState<string | null>(null);
  const [focusedCrate, setFocusedCrate] = useState<string | null>(null);

  const [selectedKeys, setSelectedKeys] = useState<Set<string>>(
    () => new Set(),
  );

  // Select all runs when observation changes or on first load (render-time adjustment)
  const [prevObservationId, setPrevObservationId] = useState(observationId);
  if (observationId !== prevObservationId) {
    setPrevObservationId(observationId);
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
    // eslint-disable-next-line react-hooks/preserve-manual-memoization
    (displayIdx: number) => {
      const origIdx = displayedOriginalIndices[displayIdx];
      if (origIdx == null) return;
      const key = runKey(runs[origIdx]);
      setSelectedKeys((prev) => {
        const next = new Set(prev);
        if (next.has(key)) next.delete(key);
        else next.add(key);
        return next;
      });
    },
    [displayedOriginalIndices, runs],
  );

  const handleSelectAllDisplayed = () => {
    setSelectedKeys((prev) => {
      const next = new Set(prev);
      for (const origIdx of displayedOriginalIndices)
        next.add(runKey(runs[origIdx]));
      return next;
    });
  };

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

  const handleSelectNoneDisplayed = () => {
    setSelectedKeys((prev) => {
      const next = new Set(prev);
      for (const origIdx of displayedOriginalIndices)
        next.delete(runKey(runs[origIdx]));
      return next;
    });
  };

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

  const handleNodeClick = (crateName: string) => {
    setCrateFilter((prev) => (prev === crateName ? null : crateName));
  };

  const handleNodeFocus = (crateName: string | null) => {
    setFocusedCrate((prev) => (prev === crateName ? null : crateName));
  };

  const handleBenchmarkCrate = useCallback((crateName: string) => {
    setBenchmarkModal({ open: true, initialCrate: crateName });
  }, []);

  // ── Loading / error guards (all hooks are above) ──────────────────────────

  if (error) {
    return (
      <div
        className="flex items-center justify-center h-full text-[11px] font-mono gap-[6px]"
        style={{ color: C.red }}
      >
        <span className="font-bold">error:</span> {error}
      </div>
    );
  }

  if (loading || !observation) {
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
            {observation.name}
          </span>
          <Badge
            color={observation.profile === "dev" ? C.textMid : C.amber}
            bg={observation.profile === "dev" ? C.surface3 : C.amber + "18"}
          >
            {observation.profile}
          </Badge>
          {observation.platform && (
            <Badge color={C.cyan} bg={C.cyan + "18"}>
              {observation.platform}
            </Badge>
          )}
          {observation.pinned && (
            <span className="text-[10px] text-accent">📌 tracked</span>
          )}
        </div>
        <div className="flex items-center gap-[12px]">
          <label
            className="flex items-center gap-[5px] bg-surface2 rounded py-[3px] px-2 text-[10px] font-mono text-dim cursor-text transition-colors duration-150"
            style={{
              border: `1px solid ${threshold > 0 ? alpha(C.accent, 33) : C.border}`,
            }}
          >
            <span className="font-semibold tracking-[0.5px] uppercase text-[10px]">
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
          {currentProject && (
            <button
              onClick={() => setBenchmarkModal({ open: true })}
              title="Create benchmark from this observation"
              className="flex items-center gap-[4px] bg-transparent border-none cursor-pointer text-dim text-[10px] font-mono hover:text-accent transition-colors"
            >
              <Rocket size={12} />
              benchmark
            </button>
          )}
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
              <span className="text-[10px] text-dim font-bold tracking-[0.5px] uppercase">
                crate
              </span>
              <span
                className="inline-flex items-center gap-[3px] text-[10px] font-mono font-semibold text-accent rounded-[3px]"
                style={{
                  background: alpha(C.accent, 9),
                  border: `1px solid ${alpha(C.accent, 27)}`,
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
              <span className="text-[10px] text-dim font-mono">
                {displayedRuns.length}/{observation.runs.length}
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
            onBenchmark={currentProject ? handleBenchmarkCrate : undefined}
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
            observation={observation}
            selectedRuns={selectedRuns}
            heat={heat}
          />
        </div>
      </div>

      {benchmarkModal.open && observation && currentProject && (
        <BenchmarkCreatorModal
          observation={observation}
          projectId={currentProject.id}
          initialCrate={benchmarkModal.initialCrate}
          onClose={() => setBenchmarkModal({ open: false })}
        />
      )}
    </div>
  );
}
