import { Link } from "react-router-dom";
import { GitCommit } from "lucide-react";
import { useCommits } from "../lib/hooks";
import { C, alpha } from "../lib/colors";
import { fmtTime } from "../lib/format";
import { Badge } from "../components/Badge";
import { useProject } from "../lib/useProject";
import type { ForagerCommit } from "../lib/data";

type CommitStatus = "not-started" | "running" | "complete";

function deriveStatus(c: ForagerCommit): CommitStatus {
  if (c.measurements.length === 0) return "not-started";
  if (c.measurements.some((m) => m.status === "running" || m.status === "pending")) return "running";
  if (c.measurements.every((m) => m.status === "complete")) return "complete";
  return "not-started";
}

function statusDot(status: CommitStatus) {
  if (status === "complete") return C.green;
  if (status === "running") return C.amber;
  return C.textDim;
}

function statusBadge(status: CommitStatus) {
  if (status === "complete")
    return { color: C.green, bg: alpha(C.green, 9), label: "complete" };
  if (status === "running")
    return { color: C.amber, bg: alpha(C.amber, 9), label: "running" };
  return { color: C.textDim, bg: C.surface3, label: "not started" };
}

const GRID = "grid grid-cols-[16px_74px_1fr_130px_86px_78px_104px] gap-[8px]";

export default function CommitsListPage() {
  const { commits, loading, error } = useCommits();
  const { current } = useProject();

  return (
    <div className="flex-1 overflow-y-auto bg-bg">
      <div className="max-w-[900px] mx-auto p-[16px] flex flex-col gap-[12px]">
        <div className="flex items-center justify-between border border-[var(--c-border)] rounded-md bg-surface px-[12px] py-[10px]">
          <div className="flex items-center gap-[8px]">
            <GitCommit size={14} color={C.accent} />
            <span className="text-xs font-mono text-accent font-bold tracking-[0.4px] uppercase">
              Commits
            </span>
          </div>
          <span className="text-[10px] text-dim font-mono">
            {commits.length} total
          </span>
        </div>

        <div className="border border-[var(--c-border)] rounded-md overflow-hidden bg-surface">
          <div
            className={`${GRID} px-[12px] py-[8px] text-[10px] font-bold text-dim uppercase tracking-[0.8px] border-b border-[var(--c-border)] bg-surface2`}
          >
            <span />
            <span>Commit</span>
            <span>Message</span>
            <span>Author</span>
            <span>Time</span>
            <span className="text-right">Measures</span>
            <span>Status</span>
          </div>

          {loading && (
            <div className="px-[12px] py-[18px] text-[11px] text-dim font-mono">
              loading commits…
            </div>
          )}

          {error && !loading && (
            <div className="px-[12px] py-[18px] text-[11px] text-c-red font-mono">
              failed to load commits: {error}
            </div>
          )}

          {!loading && !error && commits.length === 0 && (
            <div className="px-[12px] py-[18px] text-[11px] text-dim font-mono">
              no commits yet
            </div>
          )}

          {!loading &&
            !error &&
            commits.map((c) => {
              const status = deriveStatus(c);
              const badge = statusBadge(status);

              return (
                <Link
                  key={c.sha}
                  to={
                    current
                      ? `/project/${current.id}/commit/${c.shortSha}`
                      : "/"
                  }
                  className={`${GRID} px-[12px] py-[8px] items-center no-underline text-fg border-b border-[var(--c-border)]`}
                >
                  <span
                    className="w-[8px] h-[8px] rounded-full shrink-0"
                    style={{
                      background: statusDot(status),
                      boxShadow: `0 0 0 1px ${C.border}`,
                    }}
                  />
                  <span className="font-mono text-c-pink text-[11px] font-semibold">
                    {c.shortSha}
                  </span>
                  <span
                    title={c.message}
                    className="overflow-hidden text-ellipsis whitespace-nowrap text-xs"
                  >
                    {c.message}
                  </span>
                  <span className="overflow-hidden text-ellipsis whitespace-nowrap text-[11px] text-c-cyan font-mono">
                    {c.author}
                  </span>
                  <span className="text-[10px] text-dim font-mono">
                    {fmtTime(c.timestamp)}
                  </span>
                  <span className="flex justify-end">
                    <Badge color={C.textMid} bg={C.surface2}>
                      {c.measurements.length}
                    </Badge>
                  </span>
                  <span>
                    <Badge color={badge.color} bg={badge.bg}>
                      {badge.label}
                    </Badge>
                  </span>
                </Link>
              );
            })}
        </div>
      </div>
    </div>
  );
}
