import { useParams, Link } from "react-router-dom";
import { GitBranch } from "lucide-react";
import { useBranchTimeline } from "../lib/hooks";
import { C } from "../lib/colors";
import { fmtTime, fmtValue } from "../lib/format";
import { DeltaBadge } from "../components/DeltaBadge";
import { useProject } from "../lib/useProject";
import { type Measurement, measurementKey } from "../lib/data";

export default function TimelinePage() {
  const { branch } = useParams<{ branch: string }>();
  const { current } = useProject();
  const { timeline, loading, error } = useBranchTimeline(branch);
  const commits = timeline?.commits ?? [];

  /** The "previous" commit for delta computation is the next index (parent). */
  function findPrev(idx: number, m: Measurement): Measurement | undefined {
    if (idx + 1 >= commits.length) return undefined;
    const key = measurementKey(m);
    return commits[idx + 1].measurements.find((pm) => measurementKey(pm) === key);
  }

  return (
    <div className="flex-1 overflow-y-auto bg-bg">
      <div className="max-w-[960px] mx-auto p-[16px] flex flex-col gap-[12px]">
        {/* Header */}
        <div className="flex items-center justify-between border border-[var(--c-border)] rounded-md bg-surface px-[12px] py-[10px]">
          <div className="flex items-center gap-[8px]">
            <GitBranch size={14} color={C.accent} />
            <span className="text-xs font-mono text-accent font-bold tracking-[0.4px]">
              {branch}
            </span>
          </div>
          <span className="text-[10px] text-dim font-mono">
            {commits.length} commits
          </span>
        </div>

        {loading && (
          <div className="text-[11px] text-dim font-mono px-[12px]">
            loading…
          </div>
        )}
        {error && !loading && (
          <div className="text-[11px] text-c-red font-mono px-[12px]">
            failed: {error}
          </div>
        )}

        {!loading &&
          !error &&
          commits.map((c, idx) => (
            <div
              key={c.sha}
              className="border border-[var(--c-border)] rounded-md bg-surface overflow-hidden"
            >
              {/* Commit header */}
              <Link
                to={
                  current ? `/project/${current.id}/commit/${c.shortSha}` : "/"
                }
                className="flex items-center gap-[10px] px-[12px] py-[8px] no-underline border-b border-[var(--c-border)] bg-surface2"
              >
                <span className="font-mono text-c-pink text-[11px] font-semibold shrink-0">
                  {c.shortSha}
                </span>
                <span className="text-xs text-fg overflow-hidden text-ellipsis whitespace-nowrap flex-1">
                  {c.message}
                </span>
                <span className="text-[10px] text-c-cyan font-mono shrink-0">
                  {c.author}
                </span>
                <span className="text-[10px] text-dim font-mono shrink-0">
                  {fmtTime(c.timestamp)}
                </span>
              </Link>
              {/* Measurements with deltas */}
              {c.measurements.length > 0 ? (
                <div className="px-[12px] py-[8px] flex flex-col gap-[4px]">
                  {c.measurements.map((m) => {
                    const prev = findPrev(idx, m);
                    return (
                      <div
                        key={m.id}
                        className="flex items-center gap-[8px] text-[11px] font-mono py-[4px]"
                      >
                        <span className="text-mid w-[220px] overflow-hidden text-ellipsis whitespace-nowrap shrink-0">
                          {m.name}
                        </span>
                        <span className="text-fg font-semibold shrink-0">
                          {m.value != null ? fmtValue(m.value, m.unit) : "—"}
                        </span>
                        {m.value != null && prev?.value != null && (
                          <DeltaBadge
                            current={m.value}
                            baseline={prev.value}
                            unit={m.unit}
                          />
                        )}
                      </div>
                    );
                  })}
                </div>
              ) : (
                <div className="px-[12px] py-[8px] text-[11px] text-dim font-mono">
                  no measurements
                </div>
              )}
            </div>
          ))}

        {!loading && !error && commits.length === 0 && (
          <div className="text-[11px] text-dim font-mono px-[12px]">
            no commits on this branch
          </div>
        )}
      </div>
    </div>
  );
}
