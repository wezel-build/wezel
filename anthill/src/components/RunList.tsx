import { useState, useCallback, useRef, useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { useProject } from "../lib/useProject";
import { useTheme } from "../lib/theme";
import { MONO } from "../lib/format";
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
  const { C } = useTheme();
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

  const colStyle = (i: number): React.CSSProperties => ({
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    position: "relative",
    paddingRight: i < RUN_COLS.length - 1 ? 6 : 0,
  });

  const handle = (i: number) => (
    <div
      onMouseDown={(e) => startResize(i, e)}
      onMouseEnter={() => setHoveredHandle(i)}
      onMouseLeave={() => setHoveredHandle(null)}
      style={{
        position: "absolute",
        right: 0,
        top: 0,
        bottom: 0,
        width: 5,
        cursor: "col-resize",
        zIndex: 1,
        background: hoveredHandle === i ? C.accent + "44" : "transparent",
        borderRadius: hoveredHandle === i ? 1 : 0,
      }}
    />
  );

  const [hoveredCommit, setHoveredCommit] = useState<number | null>(null);

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        width: "100%",
        height: "100%",
      }}
    >
      {/* Header */}
      <div
        style={{
          padding: "4px 10px",
          borderBottom: `1px solid ${C.border}`,
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
        }}
      >
        <span
          style={{
            fontSize: 9,
            fontWeight: 700,
            color: C.textDim,
            letterSpacing: 0.8,
            textTransform: "uppercase",
          }}
        >
          Runs ({selectedIndices.size}/{runs.length})
        </span>
        <button
          onClick={allSelected ? onSelectNone : onSelectAll}
          style={{
            background: "none",
            border: `1px solid ${C.border}`,
            borderRadius: 3,
            padding: "1px 6px",
            cursor: "pointer",
            color: C.textMid,
            fontSize: 9,
            fontFamily: MONO,
          }}
        >
          {allSelected ? "none" : "all"}
        </button>
      </div>

      {/* Column headers */}
      <div
        style={{
          display: "grid",
          gridTemplateColumns: template,
          padding: "3px 10px",
          fontSize: 8,
          fontWeight: 700,
          color: C.textDim,
          textTransform: "uppercase",
          letterSpacing: 0.6,
          borderBottom: `1px solid ${C.border}`,
        }}
      >
        {RUN_COLS.map((col, i) => (
          <div key={col.key} style={colStyle(i)}>
            {col.label}
            {i < RUN_COLS.length - 1 && handle(i)}
          </div>
        ))}
      </div>

      {/* Run rows */}
      <div ref={runRowsRef} style={{ flex: 1, overflowY: "auto" }}>
        {runs.map((run, rowIdx) => {
          const isSel = selectedIndices.has(rowIdx);
          const isHl = rowIdx === hlIdx;
          const isMarked = markedIndices?.has(rowIdx) ?? false;
          const isHovered = hoveredRow === rowIdx;

          const rowBg = isHl
            ? C.accent + "22"
            : isMarked
              ? C.accent + "33"
              : isSel
                ? C.accent + "10"
                : isHovered
                  ? C.surface2
                  : "transparent";

          return (
            <div
              key={rowIdx}
              onClick={() => onToggle(rowIdx)}
              onMouseEnter={() => setHoveredRow(rowIdx)}
              onMouseLeave={() => setHoveredRow(null)}
              style={{
                display: "grid",
                gridTemplateColumns: template,
                padding: "3px 10px",
                alignItems: "center",
                cursor: "pointer",
                background: rowBg,
                borderLeft: isHl
                  ? `2px solid ${C.accent}`
                  : isMarked
                    ? `2px solid ${C.accent}`
                    : isSel
                      ? `2px solid ${C.accent}55`
                      : "2px solid transparent",
                outline: isHl ? `1px solid ${C.accent}44` : "none",
                outlineOffset: -1,
                fontSize: 10,
                fontFamily: MONO,
              }}
            >
              {/* Checkbox */}
              <div style={colStyle(0)}>
                <div
                  style={{
                    width: 12,
                    height: 12,
                    borderRadius: 2,
                    border: `1.5px solid ${isSel ? C.accent : C.border}`,
                    background: isSel ? C.accent + "33" : "transparent",
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "center",
                    fontSize: 8,
                    color: C.accent,
                  }}
                >
                  {isSel ? "✓" : ""}
                </div>
              </div>
              {/* User */}
              <div style={{ ...colStyle(1), color: C.cyan }}>{run.user}</div>
              {/* Commit */}
              <div
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
              <div style={{ ...colStyle(3), color: C.textDim }}>
                {fmtTime(run.timestamp)}
              </div>
              {/* Build time */}
              <div style={{ ...colStyle(4), color: C.textMid }}>
                {fmtMs(run.buildTimeMs)}
              </div>
              {/* Dirty count */}
              <div
                style={{
                  ...colStyle(5),
                  color: C.amber,
                  fontSize: 9,
                  textAlign: "right",
                }}
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
