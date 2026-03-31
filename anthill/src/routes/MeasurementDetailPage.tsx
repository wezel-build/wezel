import { useMemo, useState } from "react";
import { useParams, useNavigate } from "react-router-dom";
import { useKeyboardNav } from "../lib/useKeyboardNav";
import { ArrowLeft, ArrowUpDown, ArrowUp, ArrowDown } from "lucide-react";
import { C, alpha } from "../lib/colors";
import { fmtValue } from "../lib/format";
import {
  type Measurement,
  type MeasurementDetail,
  buildVizMap,
} from "../lib/data";
import { useCommits, usePheromones } from "../lib/hooks";
import { useProject } from "../lib/useProject";
import { VizRenderer } from "../components/VizRenderer";

// ── Sort logic ───────────────────────────────────────────────────────────────

type SortKey = "name" | "value" | "prev" | "delta" | "pct";
type SortDir = "asc" | "desc";

function sortDetails(
  items: MeasurementDetail[],
  key: SortKey,
  dir: SortDir,
): MeasurementDetail[] {
  const sorted = [...items];
  const m = dir === "asc" ? 1 : -1;
  sorted.sort((a, b) => {
    switch (key) {
      case "name":
        return m * a.name.localeCompare(b.name);
      case "value":
        return m * (a.value - b.value);
      default:
        return 0;
    }
  });
  return sorted;
}

// ── Bar component ────────────────────────────────────────────────────────────

function ValueBar({
  value,
  max,
  color,
}: {
  value: number;
  max: number;
  color: string;
}) {
  const pct = max > 0 ? Math.min((value / max) * 100, 100) : 0;
  return (
    <div
      className="flex-1 h-[10px] rounded-sm overflow-hidden"
      style={{ background: alpha(color, 8) }}
    >
      <div
        style={{
          width: `${pct}%`,
          height: "100%",
          background: color,
          borderRadius: 2,
          transition: "width 0.2s",
        }}
      />
    </div>
  );
}

// ── Sort header ──────────────────────────────────────────────────────────────

function SortHeader({
  label,
  sortKey,
  currentKey,
  currentDir,
  onSort,
  align,
}: {
  label: string;
  sortKey: SortKey;
  currentKey: SortKey;
  currentDir: SortDir;
  onSort: (k: SortKey) => void;
  align?: "left" | "right";
}) {
  const active = currentKey === sortKey;
  return (
    <button
      onClick={() => onSort(sortKey)}
      className="bg-transparent border-0 p-0 cursor-pointer flex items-center gap-[3px] text-[10px] font-bold font-mono uppercase tracking-[0.8px]"
      style={{
        justifyContent: align === "right" ? "flex-end" : "flex-start",
        color: active ? C.accent : C.textDim,
      }}
    >
      {label}
      {active ? (
        currentDir === "desc" ? (
          <ArrowDown size={9} />
        ) : (
          <ArrowUp size={9} />
        )
      ) : (
        <ArrowUpDown size={9} className="opacity-40" />
      )}
    </button>
  );
}

// ── Page ─────────────────────────────────────────────────────────────────────

export default function MeasurementDetailPage() {
  const { sha, id } = useParams();
  const navigate = useNavigate();

  const [sortKey, setSortKey] = useState<SortKey>("value");
  const [sortDir, setSortDir] = useState<SortDir>("desc");
  const [hoveredIdx, setHoveredIdx] = useState<number | null>(null);

  const { commits, error } = useCommits();
  const { current } = useProject();
  const { pheromones } = usePheromones();
  const vizMap = useMemo(() => buildVizMap(pheromones), [pheromones]);

  const commit = useMemo(
    () => commits.find((c) => c.shortSha === sha || c.sha === sha) ?? null,
    [sha, commits],
  );

  const measurement: Measurement | null = useMemo(
    () => commit?.measurements.find((m) => m.id === Number(id)) ?? null,
    [commit, id],
  );

  const handleSort = (key: SortKey) => {
    if (key === sortKey) {
      setSortDir((d) => (d === "desc" ? "asc" : "desc"));
    } else {
      setSortKey(key);
      setSortDir("desc");
    }
  };

  const navKeyMap = useMemo(
    () => ({
      Escape: () =>
        navigate(
          commit
            ? `/project/${current?.id}/commit/${commit.shortSha}`
            : `/project/${current?.id}/commits`,
        ),
    }),
    [commit, current?.id, navigate],
  );

  useKeyboardNav(navKeyMap);

  const sorted = useMemo(
    () =>
      measurement?.detail
        ? sortDetails(measurement.detail, sortKey, sortDir)
        : [],
    [measurement, sortKey, sortDir],
  );

  const maxValue = useMemo(
    () => Math.max(...sorted.map((d) => d.value), 1),
    [sorted],
  );

  if (!commit || !measurement) {
    return (
      <div className="flex-1 flex flex-col items-center justify-center gap-[12px] text-dim">
        <span className="text-sm font-mono">
          {!commit ? (
            <>
              commit <span style={{ color: C.red }}>{sha}</span> not found
            </>
          ) : (
            <>
              measurement <span style={{ color: C.red }}>#{id}</span> not found
            </>
          )}
        </span>
        <button
          onClick={() =>
            navigate(
              commit
                ? `/project/${current?.id}/commit/${commit.shortSha}`
                : `/project/${current?.id}/commits`,
            )
          }
          className="text-accent text-[11px] font-mono bg-transparent border-0 cursor-pointer"
        >
          ← back
        </button>
      </div>
    );
  }

  if (!measurement.detail || measurement.detail.length === 0) {
    return (
      <div className="flex-1 flex flex-col items-center justify-center gap-[12px] text-dim">
        <span className="text-[13px] font-mono">
          no detail breakdown for this measurement
        </span>
        <button
          onClick={() =>
            navigate(`/project/${current?.id}/commit/${commit.shortSha}`)
          }
          className="text-accent text-[11px] font-mono bg-transparent border-0 cursor-pointer"
        >
          ← back to {commit.shortSha}
        </button>
      </div>
    );
  }

  return (
    <div className="flex-1 flex flex-col overflow-hidden">
      {error && (
        <div
          className="px-[16px] py-[8px] text-c-red text-[11px] font-mono border-b"
          style={{
            background: C.red + "18",
            borderColor: C.red + "44",
          }}
        >
          Error: {error}
        </div>
      )}

      {/* Nav */}
      <div className="flex items-center justify-between px-[16px] py-[6px] border-b border-[var(--c-border)] shrink-0">
        <button
          onClick={() =>
            navigate(`/project/${current?.id}/commit/${commit.shortSha}`)
          }
          className="flex items-center gap-[4px] bg-transparent border-0 text-mid hover:text-accent text-[10px] font-mono cursor-pointer"
        >
          <ArrowLeft size={12} /> {commit.shortSha}
        </button>
        <span className="text-[10px] font-mono text-dim">
          {sorted.length} entries
        </span>
      </div>

      {/* Header */}
      <div className="px-[16px] py-[12px] border-b border-[var(--c-border)] flex items-center justify-between shrink-0 flex-wrap gap-[8px]">
        <div className="flex flex-col gap-[2px]">
          <span className="text-sm font-semibold font-mono text-fg">
            {measurement.name}
          </span>
          <span className="text-[10px] text-dim font-mono">
            {measurement.kind}
          </span>
        </div>
        {measurement.value != null && (
          <div className="flex items-baseline gap-[6px]">
            <span className="text-xl font-bold font-mono text-fg">
              {fmtValue(measurement.value, measurement.unit)}
            </span>
            {measurement.unit && (
              <span className="text-[10px] text-dim">{measurement.unit}</span>
            )}
          </div>
        )}
      </div>

      {/* Custom viz */}
      {vizMap[measurement.kind]?.detail && (
        <div className="px-[16px] py-[12px] border-b border-[var(--c-border)]">
          <VizRenderer
            spec={vizMap[measurement.kind]!.detail!}
            data={sorted.map((d) => ({
              name: d.name,
              value: d.value,
            }))}
            unit={measurement.unit}
          />
        </div>
      )}

      {/* Table */}
      <div className="flex-1 overflow-y-auto">
        <div className="max-w-[920px] mx-auto px-[16px]">
          {/* Column headers */}
          <div
            className="grid gap-[8px] px-[12px] py-[8px] border-b border-[var(--c-border)] sticky top-0 bg-bg z-[1]"
            style={{
              gridTemplateColumns: "1fr 80px minmax(100px, 200px)",
            }}
          >
            <SortHeader
              label="Symbol"
              sortKey="name"
              currentKey={sortKey}
              currentDir={sortDir}
              onSort={handleSort}
            />
            <SortHeader
              label="Value"
              sortKey="value"
              currentKey={sortKey}
              currentDir={sortDir}
              onSort={handleSort}
              align="right"
            />
            {/* Bar column — no sort header */}
            <span />
          </div>

          {/* Rows */}
          {sorted.map((d, i) => (
            <div
              key={i}
              className="grid gap-[8px] px-[12px] py-[8px] items-center text-[11px] font-mono"
              style={{
                gridTemplateColumns: "1fr 80px minmax(100px, 200px)",
                borderBottom: `1px solid ${alpha(C.border, 13)}`,
                background: hoveredIdx === i ? C.surface2 : "transparent",
              }}
              onMouseEnter={() => setHoveredIdx(i)}
              onMouseLeave={() => setHoveredIdx(null)}
            >
              {/* Name */}
              <span
                className="text-fg overflow-hidden text-ellipsis whitespace-nowrap text-[11px]"
                title={d.name}
              >
                {d.name}
              </span>

              {/* Value */}
              <span className="text-mid text-right font-semibold">
                {fmtValue(d.value, measurement.unit)}
              </span>

              {/* Bar */}
              <ValueBar value={d.value} max={maxValue} color={C.accent} />
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
