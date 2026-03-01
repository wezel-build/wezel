import { useState, useCallback, useRef, useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { useTheme } from "../lib/theme";
import { MONO } from "../lib/format";
import { fmtMs, fmtTime } from "../lib/format";
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
  const dragging = useRef<{
    col: number;
    startX: number;
    startW: number;
  } | null>(null);

  const onMouseDown = useCallback(
    (col: number, e: React.MouseEvent) => {
      e.preventDefault();
      dragging.current = { col, startX: e.clientX, startW: widths[col] };

      const onMouseMove = (ev: MouseEvent) => {
        if (!dragging.current) return;
        const diff = ev.clientX - dragging.current.startX;
        const newW = Math.max(20, dragging.current.startW + diff);
        setWidths((prev) => {
          const next = [...prev];
          next[dragging.current!.col] = newW;
          return next;
        });
      };

      const onMouseUp = () => {
        dragging.current = null;
        document.removeEventListener("mousemove", onMouseMove);
        document.removeEventListener("mouseup", onMouseUp);
        document.body.style.cursor = "";
        document.body.style.userSelect = "";
      };

      document.addEventListener("mousemove", onMouseMove);
      document.addEventListener("mouseup", onMouseUp);
      document.body.style.cursor = "col-resize";
      document.body.style.userSelect = "none";
    },
    [widths],
  );

  const template = widths.map((w) => `${w}px`).join(" ");
  return { widths, template, onMouseDown };
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
  const allSelected = selectedIndices.size === runs.length;
  const runRowsRef = useRef<HTMLDivElement>(null);

  // Scroll highlighted row into view
  useEffect(() => {
    if (hlIdx < 0) return;
    const container = runRowsRef.current;
    if (!container) return;
    const row = container.children[hlIdx] as HTMLElement | undefined;
    row?.scrollIntoView({ block: "nearest" });
  }, [hlIdx]);
  const { template, onMouseDown } = useResizableColumns(
    RUN_COLS.map((c) => c.init),
  );

  const colStyle = (i: number): React.CSSProperties => ({
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
    position: "relative",
    paddingRight: i < RUN_COLS.length - 1 ? 6 : 0,
  });

  const handle = (i: number) => (
    <div
      onMouseDown={(e) => onMouseDown(i, e)}
      style={{
        position: "absolute",
        right: 0,
        top: 0,
        bottom: 0,
        width: 5,
        cursor: "col-resize",
        zIndex: 1,
      }}
      onMouseEnter={(e) => (
        (e.currentTarget.style.background = C.accent + "44"),
        (e.currentTarget.style.borderRadius = "1px")
      )}
      onMouseLeave={(e) => (e.currentTarget.style.background = "transparent")}
    />
  );

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
          return (
            <div
              key={rowIdx}
              onClick={() => onToggle(rowIdx)}
              style={{
                display: "grid",
                gridTemplateColumns: template,
                padding: "3px 10px",
                alignItems: "center",
                cursor: "pointer",
                background: isHl
                  ? C.accent + "22"
                  : isMarked
                    ? C.accent + "33"
                    : isSel
                      ? C.accent + "10"
                      : "transparent",
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
              onMouseEnter={(e) => {
                if (!isSel) e.currentTarget.style.background = C.surface2;
              }}
              onMouseLeave={(e) => {
                if (!isSel)
                  e.currentTarget.style.background = isHl
                    ? C.surface2
                    : "transparent";
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
                  cursor: "pointer",
                }}
                onClick={(e) => {
                  e.stopPropagation();
                  navigate(`/commit/${run.commit}`);
                }}
                onMouseEnter={(e) =>
                  (e.currentTarget.style.textDecoration = "underline")
                }
                onMouseLeave={(e) =>
                  (e.currentTarget.style.textDecoration = "none")
                }
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
