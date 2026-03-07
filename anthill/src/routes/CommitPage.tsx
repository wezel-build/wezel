import { useMemo, useState } from "react";
import { useKeyboardNav } from "../lib/useKeyboardNav";
import {
  useParams,
  Link,
  useNavigate,
  type NavigateFunction,
} from "react-router-dom";
import {
  ArrowLeft,
  GitCommit,
  Clock,
  CheckCircle2,
  Loader,
  AlertCircle,
  Circle,
  ChevronLeft,
  ChevronRight,
  ExternalLink,
  Play,
} from "lucide-react";
import { useTheme } from "../lib/theme";
import { fmtValue, fmtTime } from "../lib/format";
import {
  type ForagerCommit,
  type Measurement,
  type MeasurementStatus,
} from "../lib/data";
import { useCommits, useGithubCommit } from "../lib/hooks";
import { useProject } from "../lib/useProject";
import { Badge } from "../components/Badge";
import { DeltaBadge } from "../components/DeltaBadge";

// ── Small pieces ─────────────────────────────────────────────────────────────

function StatusIcon({
  status,
  C,
}: {
  status: MeasurementStatus;
  C: ReturnType<typeof useTheme>["C"];
}) {
  switch (status) {
    case "complete":
      return <CheckCircle2 size={14} color={C.green} />;
    case "running":
      return (
        <Loader
          size={14}
          color={C.amber}
          style={{ animation: "spin 1.5s linear infinite" }}
        />
      );
    case "pending":
      return <Clock size={14} color={C.textDim} />;
    case "not-started":
      return <Circle size={14} color={C.textDim} className="opacity-40" />;
    case "failed":
      return <AlertCircle size={14} color={C.red} />;
  }
}

function statusLabel(s: MeasurementStatus): string {
  if (s === "not-started") return "not started";
  return s;
}

// ── Progress bar ─────────────────────────────────────────────────────────────

function ProgressBar({
  measurements,
  C,
}: {
  measurements: Measurement[];
  C: ReturnType<typeof useTheme>["C"];
}) {
  const total = measurements.length;
  if (total === 0) return null;

  const complete = measurements.filter((m) => m.status === "complete").length;
  const running = measurements.filter((m) => m.status === "running").length;
  const failed = measurements.filter((m) => m.status === "failed").length;
  const pending = measurements.filter((m) => m.status === "pending").length;
  const notStarted = measurements.filter(
    (m) => m.status === "not-started",
  ).length;

  const pct = (n: number) => `${(n / total) * 100}%`;

  return (
    <div className="flex flex-col gap-[4px]">
      <div className="flex h-[6px] rounded-[3px] overflow-hidden bg-surface3">
        <div
          style={{
            width: pct(complete),
            background: C.green,
            transition: "width 0.3s",
          }}
        />
        <div
          style={{
            width: pct(running),
            background: C.amber,
            transition: "width 0.3s",
          }}
        />
        <div
          style={{
            width: pct(failed),
            background: C.red,
            transition: "width 0.3s",
          }}
        />
      </div>
      <div className="flex gap-[10px] text-[9px] text-dim font-mono">
        <span>
          {complete}/{total} complete
        </span>
        {running > 0 && (
          <span style={{ color: C.amber }}>{running} running</span>
        )}
        {pending > 0 && <span>{pending} pending</span>}
        {notStarted > 0 && <span>{notStarted} not started</span>}
        {failed > 0 && <span style={{ color: C.red }}>{failed} failed</span>}
      </div>
    </div>
  );
}

// ── Measurement row ──────────────────────────────────────────────────────────

function MeasurementRow({
  m,
  C,
  projectId,
  commitSha,
  navigate,
}: {
  m: Measurement;
  C: ReturnType<typeof useTheme>["C"];
  projectId: number;
  commitSha: string;
  navigate: NavigateFunction;
}) {
  const [hovered, setHovered] = useState(false);
  const hasDelta =
    m.status === "complete" && m.value != null && m.prevValue != null;
  const isDone = m.status === "complete" && m.value != null;
  const hasDetail = m.detail != null && m.detail.length > 0;

  return (
    <div
      onClick={
        hasDetail
          ? () =>
              navigate(`/project/${projectId}/commit/${commitSha}/m/${m.id}`)
          : undefined
      }
      className={`grid grid-cols-[18px_1fr_70px_56px_110px] gap-[8px] px-[12px] py-[8px] items-center border-b border-[var(--c-border)] text-[11px] font-mono ${hasDetail ? "cursor-pointer" : "cursor-default"}`}
      style={{
        background: hovered && hasDetail ? C.surface2 : "transparent",
      }}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
    >
      <StatusIcon status={m.status} C={C} />

      <div className="flex items-center gap-[6px] overflow-hidden">
        <span className="text-fg overflow-hidden text-ellipsis whitespace-nowrap">
          {m.name}
        </span>
        <Badge color={C.textDim} bg={C.surface3}>
          {m.kind}
        </Badge>
      </div>

      <span
        className="text-right"
        style={{ color: isDone ? C.textMid : C.textDim }}
      >
        {isDone ? fmtValue(m.value!, m.unit) : statusLabel(m.status)}
      </span>

      <span className="text-dim text-[9px]">
        {isDone && m.unit ? m.unit : ""}
      </span>

      <span>
        {hasDelta ? (
          <DeltaBadge
            current={m.value!}
            baseline={m.prevValue!}
            unit={m.unit}
          />
        ) : (
          <span className="text-dim text-[10px]">—</span>
        )}
      </span>
    </div>
  );
}

// ── Commit header ────────────────────────────────────────────────────────────

function CommitHeader({
  commit,
  C,
}: {
  commit: ForagerCommit;
  C: ReturnType<typeof useTheme>["C"];
}) {
  const isRunning = commit.status === "running";
  const isNotStarted = commit.status === "not-started";

  const completedMs = commit.measurements.filter(
    (m) => m.status === "complete" && m.value != null && m.unit === "ms",
  );

  const totalMs =
    completedMs.length > 0
      ? completedMs.reduce((s, m) => s + (m.value ?? 0), 0)
      : null;

  const totalPrevMs =
    completedMs.length > 0 && completedMs.every((m) => m.prevValue != null)
      ? completedMs.reduce((s, m) => s + (m.prevValue ?? 0), 0)
      : null;

  const regressions = commit.measurements.filter(
    (m) =>
      m.status === "complete" &&
      m.value != null &&
      m.prevValue != null &&
      m.value > m.prevValue,
  ).length;

  const improvements = commit.measurements.filter(
    (m) =>
      m.status === "complete" &&
      m.value != null &&
      m.prevValue != null &&
      m.value < m.prevValue,
  ).length;

  return (
    <div className="flex flex-col gap-[12px] px-[20px] py-[16px] bg-surface border-b border-[var(--c-border)] rounded-t-md">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-[8px]">
          <GitCommit size={16} color={C.accent} />
          <span className="text-sm font-bold font-mono text-accent tracking-[-0.3px]">
            {commit.shortSha}
          </span>
          <Badge
            color={isNotStarted ? C.textDim : isRunning ? C.amber : C.green}
            bg={
              isNotStarted
                ? C.surface3
                : isRunning
                  ? C.amber + "18"
                  : C.green + "18"
            }
          >
            {isNotStarted
              ? "not started"
              : isRunning
                ? "in-flight"
                : "complete"}
          </Badge>
        </div>
        <span className="text-[10px] font-mono text-dim">
          {fmtTime(commit.timestamp)}
        </span>
      </div>

      <div className="flex flex-col gap-[4px]">
        <span className="text-[13px] text-fg font-medium">
          {commit.message}
        </span>
        <span className="text-[10px] text-dim font-mono">
          by {commit.author}
        </span>
      </div>

      {isRunning && <ProgressBar measurements={commit.measurements} C={C} />}

      {commit.status === "complete" && (
        <div className="flex gap-[20px] items-end flex-wrap">
          {totalMs != null && (
            <div className="flex flex-col gap-[1px]">
              <span className="text-[9px] text-dim uppercase tracking-[0.8px] font-semibold">
                Σ timed measurements
              </span>
              <div className="flex items-center gap-[8px]">
                <span className="text-lg font-bold font-mono text-fg">
                  {fmtValue(totalMs, "ms")}
                </span>
                {totalPrevMs != null && (
                  <DeltaBadge
                    current={totalMs}
                    baseline={totalPrevMs}
                    unit="ms"
                  />
                )}
              </div>
            </div>
          )}

          <div className="flex flex-col gap-[1px]">
            <span className="text-[9px] text-dim uppercase tracking-[0.8px] font-semibold">
              Measurements
            </span>
            <span
              className="text-lg font-bold font-mono"
              style={{ color: C.pink }}
            >
              {commit.measurements.length}
            </span>
          </div>

          {regressions > 0 && (
            <div className="flex flex-col gap-[1px]">
              <span className="text-[9px] text-dim uppercase tracking-[0.8px] font-semibold">
                Regressions
              </span>
              <span
                className="text-lg font-bold font-mono"
                style={{ color: C.red }}
              >
                {regressions}
              </span>
            </div>
          )}

          {improvements > 0 && (
            <div className="flex flex-col gap-[1px]">
              <span className="text-[9px] text-dim uppercase tracking-[0.8px] font-semibold">
                Improvements
              </span>
              <span
                className="text-lg font-bold font-mono"
                style={{ color: C.green }}
              >
                {improvements}
              </span>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// ── Page ─────────────────────────────────────────────────────────────────────

export default function CommitPage() {
  const { projectId: projectIdParam, sha } = useParams();
  const projectId = Number(projectIdParam);
  const hasProjectId = Number.isFinite(projectId);

  const { C } = useTheme();
  const navigate = useNavigate();
  const { pApi } = useProject();
  const { commits } = useCommits();

  const [scheduling, setScheduling] = useState(false);
  const [scheduleError, setScheduleError] = useState<string | null>(null);

  const commit = useMemo(
    () => commits.find((c) => c.shortSha === sha || c.sha === sha) ?? null,
    [sha, commits],
  );

  const ghLookupSha = commit?.sha ?? sha;
  const {
    githubCommit,
    loading: ghLoading,
    error: ghError,
  } = useGithubCommit(ghLookupSha);

  const commitIdx = useMemo(
    () => (commit ? commits.indexOf(commit) : -1),
    [commit, commits],
  );
  const prevCommit = commitIdx > 0 ? commits[commitIdx - 1] : null;
  const nextCommit =
    commitIdx < commits.length - 1 ? commits[commitIdx + 1] : null;

  const toProjectHome = hasProjectId ? `/project/${projectId}` : "/";
  const toCommit = (s: string) =>
    hasProjectId ? `/project/${projectId}/commit/${s}` : `/commit/${s}`;

  const keyMap = useMemo(
    () => ({
      ArrowLeft: () => {
        if (prevCommit) navigate(toCommit(prevCommit.shortSha));
      },
      h: () => {
        if (prevCommit) navigate(toCommit(prevCommit.shortSha));
      },
      ArrowRight: () => {
        if (nextCommit) navigate(toCommit(nextCommit.shortSha));
      },
      l: () => {
        if (nextCommit) navigate(toCommit(nextCommit.shortSha));
      },
      Escape: () => navigate(toProjectHome),
    }),
    [prevCommit, nextCommit, navigate, toProjectHome, toCommit],
  );

  useKeyboardNav(keyMap);

  const grouped = useMemo(() => {
    if (!commit) return [];
    const groups = new Map<string, Measurement[]>();
    for (const m of commit.measurements) {
      const list = groups.get(m.kind) ?? [];
      list.push(m);
      groups.set(m.kind, list);
    }
    return Array.from(groups.entries());
  }, [commit]);

  const ghMessage = githubCommit?.message?.trim() ?? "";
  const ghTitle = ghMessage ? ghMessage.split("\n")[0] : "";
  const ghBody = ghMessage.includes("\n")
    ? ghMessage.slice(ghMessage.indexOf("\n") + 1).trim()
    : "";

  const messageTitle =
    ghTitle || commit?.message || (sha ? `commit ${sha}` : "commit");
  const messageBody = ghBody || (!ghTitle ? (commit?.message ?? "") : "");
  const metaAuthor = githubCommit?.author ?? commit?.author;
  const metaTime = githubCommit?.timestamp ?? commit?.timestamp;

  const targetSha = githubCommit?.sha ?? commit?.sha ?? sha;

  const onSchedule = async () => {
    if (!targetSha || scheduling) return;
    setScheduling(true);
    setScheduleError(null);
    try {
      const created = await pApi.scheduleCommit(targetSha);
      navigate(toCommit(created.shortSha));
    } catch (e) {
      setScheduleError(String(e));
    } finally {
      setScheduling(false);
    }
  };

  if (!commit && !githubCommit && !ghLoading) {
    return (
      <div className="flex-1 flex flex-col items-center justify-center gap-[12px] text-dim">
        <span className="text-sm font-mono">
          commit <span style={{ color: C.red }}>{sha}</span> not found
        </span>
        <Link
          to={toProjectHome}
          className="text-accent text-[11px] font-mono no-underline"
        >
          ← back to scenarios
        </Link>
      </div>
    );
  }

  return (
    <div className="flex-1 flex flex-col overflow-hidden">
      <div className="flex items-center justify-between px-[16px] py-[6px] border-b border-[var(--c-border)] shrink-0">
        <button
          onClick={() => navigate(toProjectHome)}
          className="flex items-center gap-[4px] bg-transparent border-0 text-mid hover:text-accent text-[10px] font-mono cursor-pointer p-0"
        >
          <ArrowLeft size={12} /> scenarios
        </button>

        <div className="flex items-center gap-[6px]">
          {prevCommit && (
            <button
              onClick={() => navigate(toCommit(prevCommit.shortSha))}
              className="flex items-center gap-[3px] bg-surface2 border border-[var(--c-border)] rounded-[3px] px-[8px] py-[2px] cursor-pointer text-mid text-[10px] font-mono"
            >
              <ChevronLeft size={11} /> {prevCommit.shortSha}
            </button>
          )}
          {nextCommit && (
            <button
              onClick={() => navigate(toCommit(nextCommit.shortSha))}
              className="flex items-center gap-[3px] bg-surface2 border border-[var(--c-border)] rounded-[3px] px-[8px] py-[2px] cursor-pointer text-mid text-[10px] font-mono"
            >
              {nextCommit.shortSha} <ChevronRight size={11} />
            </button>
          )}
        </div>
      </div>

      <div className="flex-1 overflow-y-auto p-[16px]">
        <div className="max-w-[860px] mx-auto flex flex-col gap-[10px]">
          <div className="border border-[var(--c-border)] rounded-md bg-surface px-[14px] py-[12px] flex flex-col gap-[8px]">
            <div className="flex items-center gap-[8px]">
              <GitCommit size={14} color={C.accent} />
              <span className="text-xs font-mono text-accent font-bold">
                {githubCommit?.shortSha ?? commit?.shortSha ?? sha}
              </span>
            </div>

            <div className="text-[13px] font-semibold text-fg">
              {messageTitle}
            </div>

            {messageBody && (
              <div className="whitespace-pre-wrap text-mid text-xs leading-[1.4]">
                {messageBody}
              </div>
            )}

            {(metaAuthor || metaTime) && (
              <div className="text-[10px] font-mono text-dim">
                {metaAuthor ? `by ${metaAuthor}` : ""}
                {metaAuthor && metaTime ? " · " : ""}
                {metaTime ? fmtTime(metaTime) : ""}
              </div>
            )}

            {ghError && (
              <div className="text-[10px] font-mono text-c-red">
                GitHub metadata unavailable: {ghError}
              </div>
            )}

            {ghLoading && (
              <div className="text-[10px] font-mono text-dim">
                loading GitHub metadata…
              </div>
            )}

            <div className="flex items-center gap-[8px]">
              {githubCommit?.htmlUrl && (
                <a
                  href={githubCommit.htmlUrl}
                  target="_blank"
                  rel="noreferrer"
                  className="inline-flex items-center gap-[6px] no-underline text-[10px] font-mono text-mid border border-[var(--c-border)] rounded px-[8px] py-[4px] bg-surface2"
                >
                  <ExternalLink size={11} />
                  View diff on GitHub
                </a>
              )}

              <button
                onClick={onSchedule}
                disabled={scheduling || !targetSha}
                className="inline-flex items-center gap-[6px] text-[10px] font-mono border border-[var(--c-border)] rounded px-[8px] py-[4px] bg-surface2"
                style={{
                  color: scheduling || !targetSha ? C.textDim : C.text,
                  cursor: scheduling || !targetSha ? "default" : "pointer",
                }}
              >
                <Play size={11} />
                {scheduling ? "Scheduling..." : "Schedule Forager run"}
              </button>
            </div>

            {scheduleError && (
              <div className="text-[10px] font-mono text-c-red">
                failed to schedule run: {scheduleError}
              </div>
            )}
          </div>

          {commit ? (
            <div className="border border-[var(--c-border)] rounded-md overflow-hidden">
              <CommitHeader commit={commit} C={C} />

              <div className="grid grid-cols-[18px_1fr_70px_56px_110px] gap-[8px] px-[12px] py-[6px] text-[8px] font-bold text-dim uppercase tracking-[0.8px] border-b border-[var(--c-border)] bg-surface2">
                <span />
                <span>Measurement</span>
                <span className="text-right">Value</span>
                <span>Unit</span>
                <span>Δ prev</span>
              </div>

              {grouped.map(([kind, measurements], gi) => (
                <div key={kind}>
                  {grouped.length > 1 && (
                    <div
                      className={`px-[12px] py-[5px] text-[9px] font-bold font-mono text-dim uppercase tracking-[0.8px] bg-surface border-b border-[var(--c-border)]${gi > 0 ? " border-t border-[var(--c-border)]" : ""}`}
                    >
                      {kind}
                    </div>
                  )}
                  {measurements.map((m) => (
                    <MeasurementRow
                      key={m.id}
                      m={m}
                      C={C}
                      projectId={projectId}
                      commitSha={commit.shortSha}
                      navigate={navigate}
                    />
                  ))}
                </div>
              ))}
            </div>
          ) : (
            <div className="border border-[var(--c-border)] rounded-md bg-surface px-[16px] py-[14px] text-dim text-[11px] font-mono">
              No Forager measurements yet for this commit.
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
