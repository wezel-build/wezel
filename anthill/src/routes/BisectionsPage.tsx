import { useState, useCallback } from "react";
import { Link } from "react-router-dom";
import { Search } from "lucide-react";
import { useBisections } from "../lib/hooks";
import { C, alpha } from "../lib/colors";
import { Badge } from "../components/Badge";
import { useProject } from "../lib/useProject";
import type { BisectionStatus } from "../lib/data";

const STATUS_OPTIONS: (BisectionStatus | "all")[] = [
  "all",
  "active",
  "complete",
  "abandoned",
];

function statusBadge(status: BisectionStatus) {
  switch (status) {
    case "active":
      return { color: C.amber, bg: alpha(C.amber, 9), label: "active" };
    case "complete":
      return { color: C.green, bg: alpha(C.green, 9), label: "complete" };
    case "abandoned":
      return { color: C.textDim, bg: C.surface3, label: "abandoned" };
  }
}

const GRID =
  "grid grid-cols-[90px_1fr_120px_100px_100px_90px] gap-[8px] items-center";

export default function BisectionsPage() {
  const { current, pApi } = useProject();
  const [statusFilter, setStatusFilter] = useState<BisectionStatus | "all">(
    "all",
  );
  const { bisections, loading, error, refetch } = useBisections(
    statusFilter === "all" ? undefined : statusFilter,
  );

  const abandon = useCallback(
    async (id: number) => {
      await pApi.abandonBisection(id);
      refetch();
    },
    [pApi, refetch],
  );

  return (
    <div className="flex-1 overflow-y-auto bg-bg">
      <div className="max-w-[960px] mx-auto p-[16px] flex flex-col gap-[12px]">
        {/* Header */}
        <div className="flex items-center justify-between border border-[var(--c-border)] rounded-md bg-surface px-[12px] py-[10px]">
          <div className="flex items-center gap-[8px]">
            <Search size={14} color={C.accent} />
            <span className="text-xs font-mono text-accent font-bold tracking-[0.4px] uppercase">
              Bisections
            </span>
          </div>
          <div className="flex items-center gap-[6px]">
            {STATUS_OPTIONS.map((s) => (
              <button
                key={s}
                onClick={() => setStatusFilter(s)}
                className="text-[10px] font-mono font-semibold px-[6px] py-[2px] rounded border cursor-pointer"
                style={{
                  color: statusFilter === s ? C.accent : C.textDim,
                  background:
                    statusFilter === s ? alpha(C.accent, 10) : "transparent",
                  borderColor:
                    statusFilter === s ? alpha(C.accent, 25) : C.border,
                }}
              >
                {s}
              </button>
            ))}
          </div>
        </div>

        {/* Table */}
        <div className="border border-[var(--c-border)] rounded-md overflow-hidden bg-surface">
          <div
            className={`${GRID} px-[12px] py-[6px] text-[8px] font-bold text-dim uppercase tracking-[0.8px] border-b border-[var(--c-border)] bg-surface2`}
          >
            <span>Status</span>
            <span>Benchmark / Measurement</span>
            <span>Branch</span>
            <span className="text-right">Good</span>
            <span className="text-right">Bad</span>
            <span>Culprit</span>
          </div>

          {loading && (
            <div className="px-[12px] py-[18px] text-[11px] text-dim font-mono">
              loading…
            </div>
          )}
          {error && !loading && (
            <div className="px-[12px] py-[18px] text-[11px] text-c-red font-mono">
              failed: {error}
            </div>
          )}
          {!loading && !error && bisections.length === 0 && (
            <div className="px-[12px] py-[18px] text-[11px] text-dim font-mono">
              no bisections
            </div>
          )}

          {!loading &&
            !error &&
            bisections.map((b) => {
              const badge = statusBadge(b.status);
              return (
                <div
                  key={b.id}
                  className={`${GRID} px-[12px] py-[7px] border-b border-[var(--c-border)] text-[11px] font-mono`}
                >
                  <span className="flex items-center gap-[6px]">
                    <Badge color={badge.color} bg={badge.bg}>
                      {badge.label}
                    </Badge>
                  </span>
                  <Link
                    to={
                      current
                        ? `/project/${current.id}/bisections/${b.id}`
                        : "#"
                    }
                    className="no-underline text-fg overflow-hidden text-ellipsis whitespace-nowrap"
                  >
                    <span className="font-semibold">{b.benchmarkName}</span>
                    <span className="text-dim"> / </span>
                    <span className="text-mid">{b.measurementName}</span>
                  </Link>
                  <span className="text-c-cyan">{b.branch}</span>
                  <span className="text-right text-c-green">
                    {b.goodValue.toLocaleString()}
                  </span>
                  <span className="text-right text-c-red">
                    {b.badValue.toLocaleString()}
                  </span>
                  <span>
                    {b.culpritSha ? (
                      <Link
                        to={
                          current
                            ? `/project/${current.id}/commit/${b.culpritSha.slice(0, 7)}`
                            : "#"
                        }
                        className="font-semibold text-c-pink no-underline"
                      >
                        {b.culpritSha.slice(0, 7)}
                      </Link>
                    ) : b.status === "active" ? (
                      <button
                        onClick={() => abandon(b.id)}
                        className="text-[10px] font-mono text-dim bg-transparent border border-[var(--c-border)] rounded px-[5px] py-[1px] cursor-pointer"
                      >
                        abandon
                      </button>
                    ) : (
                      <span className="text-dim">—</span>
                    )}
                  </span>
                </div>
              );
            })}
        </div>
      </div>
    </div>
  );
}
