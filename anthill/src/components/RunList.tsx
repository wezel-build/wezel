import { useState, useCallback, useRef, useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { useProject } from "../lib/useProject";
import { C, alpha } from "../lib/colors";
import { fmtMs, fmtTime } from "../lib/format";
import { useDrag } from "../lib/useDrag";
import type { Run } from "../lib/data";

const RUN_COLS = [
  { key: "sel", label: "✓", init: 20 },
  { key: "user", label: "User", init: 42 },
  { key: "commit", label: "Commit", init: 54 },
  { key: "time", label: "Time", init: 72 },
  { key: "build", label: "Build", init: 44 },
  { key: "dirty", label: "Δ", init: 20 },
];

function useResizableColumns(initial: number[]) {
  const [widths, setWidths] = useState(initial);
  const [activeCol, setActiveCol] = useState<number | null>(null);
  const [hoveredHandle, setHoveredHandle] = useState<number | null>(null);

  const activeColRef = useRef(activeCol);
  useEffect(() => {
    activeColRef.current = activeCol;
  }, [activeCol]);

  const onDragMove = useCallback((dx: number) => {
    const col = activeColRef.current;
    if (col == null) return;
    setWidths((prev) => {
      const next = [...prev];
      next[col] = Math.max(20, next[col] + dx);
      return next;
    });
  }, []);

  const onDragEnd = useCallback(() => {
    setActiveCol(null);
  }, []);

  const onMouseDown = useDrag({
    onDrag: onDragMove,
    onDragEnd,
    cursor: "col-resize",
  });

  const startResize = useCallback(
    (col: number, e: React.MouseEvent) => {
      setActiveCol(col);
      onMouseDown(e);
    },
    [onMouseDown],
  );

  const template = widths.map((w) => `${w}px`).join(" ");
  return { widths, template, startResize, hoveredHandle, setHoveredHandle };
}

export function RunList({
  runs,
  selectedIndices,
  onToggle,
  onSelectAll,
  onSelectNone,
  hlIdx = -1,
  markedIndices,
}: {
  runs: Run[];
  selectedIndices: Set<number>;
  onToggle: (i: number) => void;
  onSelectAll: () => void;
  onSelectNone: () => void;
  hlIdx?: number;
  markedIndices?: Set<number>;
}) {
  const navigate = useNavigate();
  const { current } = useProject();
  const allSelected = selectedIndices.size === runs.length;
  const runRowsRef = useRef<HTMLDivElement>(null);
  const [hoveredRow, setHoveredRow] = useState<number | null>(null);

  // Scroll highlighted row into view
  useEffect(() => {
    if (hlIdx < 0) return;
    const container = runRowsRef.current;
    if (!container) return;
    const row = container.children[hlIdx] as HTMLElement | undefined;
    row?.scrollIntoView({ block: "nearest" });
  }, [hlIdx]);
  const { template, startResize, hoveredHandle, setHoveredHandle } =
    useResizableColumns(RUN_COLS.map((c) => c.init));

  // Static parts of colStyle as a className; dynamic paddingRight stays inline
  const colClassName =
    "overflow-hidden text-ellipsis whitespace-nowrap relative";
  const colStyle = (i: number): React.CSSProperties => ({
    paddingRight: i < RUN_COLS.length - 1 ? 6 : 0,
  });

  const handle = (i: number) => (
    <div
      onMouseDown={(e) => startResize(i, e)}
      onMouseEnter={() => setHoveredHandle(i)}
      onMouseLeave={() => setHoveredHandle(null)}
      className="absolute right-0 top-0 bottom-0 w-[5px] cursor-col-resize z-[1]"
      style={{
        background: hoveredHandle === i ? alpha(C.accent, 27) : "transparent",
        borderRadius: hoveredHandle === i ? 1 : 0,
      }}
    />
  );

  const [hoveredCommit, setHoveredCommit] = useState<number | null>(null);

  return (
    <div className="flex flex-col w-full h-full">
      {/* Header */}
      <div className="py-1 px-[10px] border-b border-[var(--c-border)] flex items-center justify-between">
        <span className="text-[9px] font-bold text-dim tracking-[0.8px] uppercase">
          Runs ({selectedIndices.size}/{runs.length})
        </span>
        <button
          onClick={allSelected ? onSelectNone : onSelectAll}
          className="bg-transparent border border-[var(--c-border)] rounded-[3px] py-[1px] px-[6px] cursor-pointer text-mid text-[9px] font-mono"
        >
          {allSelected ? "none" : "all"}
        </button>
      </div>

      {/* Column headers */}
      <div
        className="grid py-[3px] px-[10px] text-[8px] font-bold text-dim uppercase tracking-[0.6px] border-b border-[var(--c-border)]"
        style={{ gridTemplateColumns: template }}
      >
        {RUN_COLS.map((col, i) => (
          <div key={col.key} className={colClassName} style={colStyle(i)}>
            {col.label}
            {i < RUN_COLS.length - 1 && handle(i)}
          </div>
        ))}
      </div>

      {/* Run rows */}
      <div ref={runRowsRef} className="flex-1 overflow-y-auto">
        {runs.map((run, rowIdx) => {
          const isSel = selectedIndices.has(rowIdx);
          const isHl = rowIdx === hlIdx;
          const isMarked = markedIndices?.has(rowIdx) ?? false;
          const isHovered = hoveredRow === rowIdx;

          const rowBg = isHl
            ? alpha(C.accent, 13)
            : isMarked
              ? alpha(C.accent, 20)
              : isSel
                ? alpha(C.accent, 6)
                : isHovered
                  ? C.surface2
                  : "transparent";

          return (
            <div
              key={rowIdx}
              onClick={() => onToggle(rowIdx)}
              onMouseEnter={() => setHoveredRow(rowIdx)}
              onMouseLeave={() => setHoveredRow(null)}
              className="grid py-[3px] px-[10px] items-center cursor-pointer text-[10px] font-mono"
              style={{
                gridTemplateColumns: template,
                background: rowBg,
                borderLeft: isHl
                  ? `2px solid ${C.accent}`
                  : isMarked
                    ? `2px solid ${C.accent}`
                    : isSel
                      ? `2px solid ${alpha(C.accent, 33)}`
                      : "2px solid transparent",
                outline: isHl ? `1px solid ${alpha(C.accent, 27)}` : "none",
                outlineOffset: -1,
              }}
            >
              {/* Checkbox */}
              <div className={colClassName} style={colStyle(0)}>
                <div
                  className="w-[12px] h-[12px] rounded-sm flex items-center justify-center text-[8px]"
                  style={{
                    border: `1.5px solid ${isSel ? C.accent : C.border}`,
                    background: isSel ? alpha(C.accent, 20) : "transparent",
                    color: C.accent,
                  }}
                >
                  {isSel ? "✓" : ""}
                </div>
              </div>
              {/* User */}
              <div
                className={colClassName}
                style={{ ...colStyle(1), color: C.cyan }}
              >
                {run.user}
              </div>
              {/* Commit */}
              <div
                className={colClassName}
                style={{
                  ...colStyle(2),
                  color: C.pink,
                  fontSize: 9,
                  cursor: run.commit ? "pointer" : "default",
                  textDecoration:
                    hoveredCommit === rowIdx && run.commit
                      ? "underline"
                      : "none",
                }}
                onClick={(e) => {
                  if (!run.commit) return;
                  e.stopPropagation();
                  navigate(`/project/${current?.id}/commit/${run.commit}`);
                }}
                onMouseEnter={() => {
                  if (run.commit) setHoveredCommit(rowIdx);
                }}
                onMouseLeave={() => setHoveredCommit(null)}
              >
                {run.commit ? run.commit.slice(0, 7) : ""}
              </div>
              {/* Timestamp */}
              <div
                className={colClassName}
                style={{ ...colStyle(3), color: C.textDim }}
              >
                {fmtTime(run.timestamp)}
              </div>
              {/* Build time */}
              <div
                className={colClassName}
                style={{ ...colStyle(4), color: C.textMid }}
              >
                {fmtMs(run.buildTimeMs)}
              </div>
              {/* Dirty count */}
              <div
                className={`${colClassName} text-[9px] text-right`}
                style={{ ...colStyle(5), color: C.amber }}
              >
                {run.dirtyCrates.length}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
