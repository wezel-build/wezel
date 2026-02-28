import { useMemo, useState } from "react";
import { useParams, useNavigate } from "react-router-dom";
import { useKeyboardNav } from "../lib/useKeyboardNav";
import { ArrowLeft, ArrowUpDown, ArrowUp, ArrowDown } from "lucide-react";
import { useTheme } from "../lib/theme";
import { MONO, fmtValue } from "../lib/format";
import {
  MOCK_COMMITS,
  type Measurement,
  type MeasurementDetail,
} from "../lib/data";

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
      case "prev":
        return m * ((a.prevValue ?? 0) - (b.prevValue ?? 0));
      case "delta":
        return (
          m *
          (a.value -
            (a.prevValue ?? a.value) -
            (b.value - (b.prevValue ?? b.value)))
        );
      case "pct": {
        const pa = a.prevValue
          ? ((a.value - a.prevValue) / a.prevValue) * 100
          : 0;
        const pb = b.prevValue
          ? ((b.value - b.prevValue) / b.prevValue) * 100
          : 0;
        return m * (pa - pb);
      }
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
      style={{
        flex: 1,
        height: 10,
        background: color + "15",
        borderRadius: 2,
        overflow: "hidden",
      }}
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
  C,
  align,
}: {
  label: string;
  sortKey: SortKey;
  currentKey: SortKey;
  currentDir: SortDir;
  onSort: (k: SortKey) => void;
  C: ReturnType<typeof useTheme>["C"];
  align?: "left" | "right";
}) {
  const active = currentKey === sortKey;
  return (
    <button
      onClick={() => onSort(sortKey)}
      style={{
        background: "none",
        border: "none",
        padding: 0,
        cursor: "pointer",
        display: "flex",
        alignItems: "center",
        gap: 3,
        justifyContent: align === "right" ? "flex-end" : "flex-start",
        color: active ? C.accent : C.textDim,
        fontSize: 8,
        fontWeight: 700,
        fontFamily: MONO,
        textTransform: "uppercase",
        letterSpacing: 0.8,
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
        <ArrowUpDown size={9} style={{ opacity: 0.4 }} />
      )}
    </button>
  );
}

// ── Page ─────────────────────────────────────────────────────────────────────

export default function MeasurementDetailPage() {
  const { sha, id } = useParams();
  const { C } = useTheme();
  const navigate = useNavigate();

  const [sortKey, setSortKey] = useState<SortKey>("value");
  const [sortDir, setSortDir] = useState<SortDir>("desc");

  const commit = useMemo(
    () => MOCK_COMMITS.find((c) => c.shortSha === sha || c.sha === sha) ?? null,
    [sha],
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
      Escape: () => navigate(commit ? `/commit/${commit.shortSha}` : "/"),
    }),
    [commit, navigate],
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
      <div
        style={{
          flex: 1,
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          justifyContent: "center",
          gap: 12,
          color: C.textDim,
        }}
      >
        <span style={{ fontSize: 14, fontFamily: MONO }}>
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
          onClick={() => navigate(commit ? `/commit/${commit.shortSha}` : "/")}
          style={{
            color: C.accent,
            fontSize: 11,
            fontFamily: MONO,
            background: "none",
            border: "none",
            cursor: "pointer",
          }}
        >
          ← back
        </button>
      </div>
    );
  }

  if (!measurement.detail || measurement.detail.length === 0) {
    return (
      <div
        style={{
          flex: 1,
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          justifyContent: "center",
          gap: 12,
          color: C.textDim,
        }}
      >
        <span style={{ fontSize: 13, fontFamily: MONO }}>
          no detail breakdown for this measurement
        </span>
        <button
          onClick={() => navigate(`/commit/${commit.shortSha}`)}
          style={{
            color: C.accent,
            fontSize: 11,
            fontFamily: MONO,
            background: "none",
            border: "none",
            cursor: "pointer",
          }}
        >
          ← back to {commit.shortSha}
        </button>
      </div>
    );
  }

  const hasPrev = sorted.some((d) => d.prevValue != null);

  return (
    <div
      style={{
        flex: 1,
        display: "flex",
        flexDirection: "column",
        overflow: "hidden",
      }}
    >
      {/* Nav */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          padding: "6px 16px",
          borderBottom: `1px solid ${C.border}`,
          flexShrink: 0,
        }}
      >
        <button
          onClick={() => navigate(`/commit/${commit.shortSha}`)}
          style={{
            display: "flex",
            alignItems: "center",
            gap: 4,
            background: "none",
            border: "none",
            color: C.textMid,
            fontSize: 10,
            fontFamily: MONO,
            cursor: "pointer",
          }}
          onMouseEnter={(e) => (e.currentTarget.style.color = C.accent)}
          onMouseLeave={(e) => (e.currentTarget.style.color = C.textMid)}
        >
          <ArrowLeft size={12} /> {commit.shortSha}
        </button>
        <span style={{ fontSize: 10, fontFamily: MONO, color: C.textDim }}>
          {sorted.length} entries
        </span>
      </div>

      {/* Header */}
      <div
        style={{
          padding: "12px 16px",
          borderBottom: `1px solid ${C.border}`,
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          flexShrink: 0,
          flexWrap: "wrap",
          gap: 8,
        }}
      >
        <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
          <span
            style={{
              fontSize: 14,
              fontWeight: 600,
              fontFamily: MONO,
              color: C.text,
            }}
          >
            {measurement.name}
          </span>
          <span style={{ fontSize: 10, color: C.textDim, fontFamily: MONO }}>
            {measurement.kind}
          </span>
        </div>
        {measurement.value != null && (
          <div
            style={{
              display: "flex",
              alignItems: "baseline",
              gap: 6,
            }}
          >
            <span
              style={{
                fontSize: 20,
                fontWeight: 700,
                fontFamily: MONO,
                color: C.text,
              }}
            >
              {fmtValue(measurement.value, measurement.unit)}
            </span>
            {measurement.unit && (
              <span style={{ fontSize: 10, color: C.textDim }}>
                {measurement.unit}
              </span>
            )}
            {measurement.prevValue != null &&
              (() => {
                const diff = measurement.value! - measurement.prevValue;
                const pct =
                  measurement.prevValue !== 0
                    ? Math.round((diff / measurement.prevValue) * 100)
                    : 0;
                const isUp = diff > 0;
                const color = isUp ? C.red : C.green;
                const sign = isUp ? "+" : "";
                return (
                  <span
                    style={{
                      fontSize: 11,
                      fontFamily: MONO,
                      fontWeight: 600,
                      color,
                      padding: "1px 6px",
                      borderRadius: 3,
                      background: color + "15",
                      border: `1px solid ${color}33`,
                    }}
                  >
                    {sign}
                    {fmtValue(diff, measurement.unit)} ({sign}
                    {pct}%)
                  </span>
                );
              })()}
          </div>
        )}
      </div>

      {/* Table */}
      <div style={{ flex: 1, overflowY: "auto" }}>
        <div style={{ maxWidth: 920, margin: "0 auto", padding: "0 16px" }}>
          {/* Column headers */}
          <div
            style={{
              display: "grid",
              gridTemplateColumns: hasPrev
                ? "1fr 80px minmax(80px, 160px) 80px 100px"
                : "1fr 80px minmax(100px, 200px)",
              gap: 8,
              padding: "8px 12px",
              borderBottom: `1px solid ${C.border}`,
              position: "sticky",
              top: 0,
              background: C.bg,
              zIndex: 1,
            }}
          >
            <SortHeader
              label="Symbol"
              sortKey="name"
              currentKey={sortKey}
              currentDir={sortDir}
              onSort={handleSort}
              C={C}
            />
            <SortHeader
              label="Value"
              sortKey="value"
              currentKey={sortKey}
              currentDir={sortDir}
              onSort={handleSort}
              C={C}
              align="right"
            />
            {/* Bar column — no sort header */}
            <span />
            {hasPrev && (
              <>
                <SortHeader
                  label="Prev"
                  sortKey="prev"
                  currentKey={sortKey}
                  currentDir={sortDir}
                  onSort={handleSort}
                  C={C}
                  align="right"
                />
                <SortHeader
                  label="Δ"
                  sortKey="delta"
                  currentKey={sortKey}
                  currentDir={sortDir}
                  onSort={handleSort}
                  C={C}
                  align="right"
                />
              </>
            )}
          </div>

          {/* Rows */}
          {sorted.map((d, i) => {
            const diff = d.prevValue != null ? d.value - d.prevValue : null;
            const pct =
              d.prevValue != null && d.prevValue !== 0
                ? Math.round(((d.value - d.prevValue) / d.prevValue) * 100)
                : null;
            const isRegression = diff != null && diff > 0;
            const deltaColor =
              diff == null
                ? C.textDim
                : diff === 0
                  ? C.textDim
                  : isRegression
                    ? C.red
                    : C.green;

            return (
              <div
                key={i}
                style={{
                  display: "grid",
                  gridTemplateColumns: hasPrev
                    ? "1fr 80px minmax(80px, 160px) 80px 100px"
                    : "1fr 80px minmax(100px, 200px)",
                  gap: 8,
                  padding: "6px 12px",
                  alignItems: "center",
                  borderBottom: `1px solid ${C.border}22`,
                  fontSize: 11,
                  fontFamily: MONO,
                }}
                onMouseEnter={(e) =>
                  (e.currentTarget.style.background = C.surface2)
                }
                onMouseLeave={(e) =>
                  (e.currentTarget.style.background = "transparent")
                }
              >
                {/* Name */}
                <span
                  style={{
                    color: C.text,
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    whiteSpace: "nowrap",
                    fontSize: 10,
                  }}
                  title={d.name}
                >
                  {d.name}
                </span>

                {/* Value */}
                <span
                  style={{
                    color: C.textMid,
                    textAlign: "right",
                    fontWeight: 600,
                  }}
                >
                  {fmtValue(d.value, measurement.unit)}
                </span>

                {/* Bar */}
                <ValueBar
                  value={d.value}
                  max={maxValue}
                  color={
                    isRegression
                      ? C.red
                      : diff != null && diff < 0
                        ? C.green
                        : C.accent
                  }
                />

                {/* Prev value */}
                {hasPrev && (
                  <span
                    style={{
                      color: C.textDim,
                      textAlign: "right",
                      fontSize: 10,
                    }}
                  >
                    {d.prevValue != null
                      ? fmtValue(d.prevValue, measurement.unit)
                      : "—"}
                  </span>
                )}

                {/* Delta */}
                {hasPrev && (
                  <span
                    style={{
                      textAlign: "right",
                      fontSize: 10,
                      fontWeight: 600,
                      color: deltaColor,
                    }}
                  >
                    {diff != null && diff !== 0
                      ? `${diff > 0 ? "+" : ""}${fmtValue(diff, measurement.unit)} (${pct! > 0 ? "+" : ""}${pct}%)`
                      : diff === 0
                        ? "—"
                        : "—"}
                  </span>
                )}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
