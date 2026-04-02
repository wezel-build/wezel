import { useParams, Link, useNavigate } from "react-router-dom";
import { ArrowLeft, Search } from "lucide-react";
import { useBisection } from "../lib/hooks";
import { C, alpha } from "../lib/colors";
import { useProject } from "../lib/useProject";
import { Badge } from "../components/Badge";
import { useCallback } from "react";

function statusBadge(status: string) {
  switch (status) {
    case "active":
      return { color: C.amber, bg: alpha(C.amber, 9), label: "active" };
    case "complete":
      return { color: C.green, bg: alpha(C.green, 9), label: "complete" };
    case "abandoned":
      return { color: C.textDim, bg: C.surface3, label: "abandoned" };
    default:
      return { color: C.textDim, bg: C.surface3, label: status };
  }
}

export default function BisectionDetailPage() {
  const { projectId, bisectionId } = useParams<{
    projectId: string;
    bisectionId: string;
  }>();
  const { current, pApi } = useProject();
  const navigate = useNavigate();
  const id = bisectionId ? Number(bisectionId) : undefined;
  const { bisection: b, loading, error } = useBisection(id);

  const abandon = useCallback(async () => {
    if (!id) return;
    await pApi.abandonBisection(id);
    navigate(`/project/${projectId}/bisections`);
  }, [id, pApi, projectId, navigate]);

  if (loading) {
    return (
      <div className="flex-1 flex items-center justify-center text-[11px] text-dim font-mono">
        loading…
      </div>
    );
  }
  if (error || !b) {
    return (
      <div className="flex-1 flex items-center justify-center text-[11px] text-c-red font-mono">
        {error ?? "bisection not found"}
      </div>
    );
  }

  const badge = statusBadge(b.status);
  const pctChange =
    b.goodValue !== 0
      ? Math.round(((b.badValue - b.goodValue) / b.goodValue) * 100)
      : 0;

  return (
    <div className="flex-1 overflow-y-auto bg-bg">
      <div className="max-w-[700px] mx-auto p-[16px] flex flex-col gap-[16px]">
        {/* Back link */}
        <Link
          to={current ? `/project/${current.id}/bisections` : "/"}
          className="flex items-center gap-[4px] text-[11px] font-mono text-dim no-underline"
        >
          <ArrowLeft size={12} /> bisections
        </Link>

        {/* Title card */}
        <div className="border border-[var(--c-border)] rounded-md bg-surface overflow-hidden">
          <div className="flex items-center gap-[10px] px-[14px] py-[10px] border-b border-[var(--c-border)] bg-surface2">
            <Search size={14} color={C.accent} />
            <span className="text-xs font-mono text-fg font-bold flex-1">
              {b.experimentName}{" "}
              <span className="text-mid font-normal">
                / {b.measurementName}
              </span>
            </span>
            <Badge color={badge.color} bg={badge.bg}>
              {badge.label}
            </Badge>
          </div>

          <div className="px-[14px] py-[12px] flex flex-col gap-[10px] text-[11px] font-mono">
            {/* Branch */}
            <Row label="Branch">
              <span className="text-c-cyan">{b.branch}</span>
            </Row>

            {/* Good / Bad range */}
            <Row label="Good">
              <CommitLink sha={b.goodSha} projectId={current?.id} />
              <span className="text-c-green ml-[8px]">
                {b.goodValue.toLocaleString()}
              </span>
            </Row>
            <Row label="Bad">
              <CommitLink sha={b.badSha} projectId={current?.id} />
              <span className="text-c-red ml-[8px]">
                {b.badValue.toLocaleString()}
              </span>
              <span className="text-dim ml-[4px]">
                ({pctChange > 0 ? "+" : ""}
                {pctChange}%)
              </span>
            </Row>

            {/* Culprit */}
            {b.culpritSha && (
              <Row label="Culprit">
                <CommitLink sha={b.culpritSha} projectId={current?.id} />
              </Row>
            )}

            {/* Compare link */}
            {current && (
              <Row label="Compare">
                <Link
                  to={`/project/${current.id}/compare?base_sha=${b.goodSha}&head_sha=${b.badSha}`}
                  className="text-accent no-underline text-[11px] font-mono"
                >
                  view diff →
                </Link>
              </Row>
            )}
          </div>
        </div>

        {/* Abandon button */}
        {b.status === "active" && (
          <button
            onClick={abandon}
            className="self-start text-[11px] font-mono text-dim bg-transparent border border-[var(--c-border)] rounded px-[8px] py-[4px] cursor-pointer"
          >
            abandon bisection
          </button>
        )}
      </div>
    </div>
  );
}

function Row({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex items-center gap-[8px]">
      <span className="text-dim w-[70px] shrink-0 text-right">{label}</span>
      {children}
    </div>
  );
}

function CommitLink({
  sha,
  projectId,
}: {
  sha: string;
  projectId: number | undefined;
}) {
  const short = sha.slice(0, 7);
  if (!projectId) return <span className="text-c-pink">{short}</span>;
  return (
    <Link
      to={`/project/${projectId}/commit/${short}`}
      className="text-c-pink no-underline font-semibold"
    >
      {short}
    </Link>
  );
}
