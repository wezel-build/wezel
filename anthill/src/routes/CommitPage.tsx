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
import { MONO, fmtValue, fmtTime } from "../lib/format";
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
      return <Circle size={14} color={C.textDim} style={{ opacity: 0.4 }} />;
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
    <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
      <div
        style={{
          display: "flex",
          height: 6,
          borderRadius: 3,
          overflow: "hidden",
          background: C.surface3,
        }}
      >
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
      <div
        style={{
          display: "flex",
          gap: 10,
          fontSize: 9,
          color: C.textDim,
          fontFamily: MONO,
        }}
      >
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
      style={{
        display: "grid",
        gridTemplateColumns: "18px 1fr 70px 56px 110px",
        gap: 8,
        padding: "8px 12px",
        alignItems: "center",
        borderBottom: `1px solid ${C.border}`,
        fontSize: 11,
        fontFamily: MONO,
        cursor: hasDetail ? "pointer" : "default",
      }}
      onMouseEnter={(e) => {
        if (hasDetail) e.currentTarget.style.background = C.surface2;
      }}
      onMouseLeave={(e) => {
        if (hasDetail) e.currentTarget.style.background = "transparent";
      }}
    >
      <StatusIcon status={m.status} C={C} />

      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 6,
          overflow: "hidden",
        }}
      >
        <span
          style={{
            color: C.text,
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}
        >
          {m.name}
        </span>
        <Badge color={C.textDim} bg={C.surface3}>
          {m.kind}
        </Badge>
      </div>

      <span
        style={{ color: isDone ? C.textMid : C.textDim, textAlign: "right" }}
      >
        {isDone ? fmtValue(m.value!, m.unit) : statusLabel(m.status)}
      </span>

      <span style={{ color: C.textDim, fontSize: 9 }}>
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
          <span style={{ color: C.textDim, fontSize: 10 }}>—</span>
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
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 12,
        padding: "16px 20px",
        background: C.surface,
        borderBottom: `1px solid ${C.border}`,
        borderRadius: "6px 6px 0 0",
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <GitCommit size={16} color={C.accent} />
          <span
            style={{
              fontSize: 14,
              fontWeight: 700,
              fontFamily: MONO,
              color: C.accent,
              letterSpacing: -0.3,
            }}
          >
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
        <span style={{ fontSize: 10, fontFamily: MONO, color: C.textDim }}>
          {fmtTime(commit.timestamp)}
        </span>
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
        <span style={{ fontSize: 13, color: C.text, fontWeight: 500 }}>
          {commit.message}
        </span>
        <span style={{ fontSize: 10, color: C.textDim, fontFamily: MONO }}>
          by {commit.author}
        </span>
      </div>

      {isRunning && <ProgressBar measurements={commit.measurements} C={C} />}

      {commit.status === "complete" && (
        <div
          style={{
            display: "flex",
            gap: 20,
            alignItems: "flex-end",
            flexWrap: "wrap",
          }}
        >
          {totalMs != null && (
            <div style={{ display: "flex", flexDirection: "column", gap: 1 }}>
              <span
                style={{
                  fontSize: 9,
                  color: C.textDim,
                  textTransform: "uppercase",
                  letterSpacing: 0.8,
                  fontWeight: 600,
                }}
              >
                Σ timed measurements
              </span>
              <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                <span
                  style={{
                    fontSize: 18,
                    fontWeight: 700,
                    fontFamily: MONO,
                    color: C.text,
                  }}
                >
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

          <div style={{ display: "flex", flexDirection: "column", gap: 1 }}>
            <span
              style={{
                fontSize: 9,
                color: C.textDim,
                textTransform: "uppercase",
                letterSpacing: 0.8,
                fontWeight: 600,
              }}
            >
              Measurements
            </span>
            <span
              style={{
                fontSize: 18,
                fontWeight: 700,
                fontFamily: MONO,
                color: C.pink,
              }}
            >
              {commit.measurements.length}
            </span>
          </div>

          {regressions > 0 && (
            <div style={{ display: "flex", flexDirection: "column", gap: 1 }}>
              <span
                style={{
                  fontSize: 9,
                  color: C.textDim,
                  textTransform: "uppercase",
                  letterSpacing: 0.8,
                  fontWeight: 600,
                }}
              >
                Regressions
              </span>
              <span
                style={{
                  fontSize: 18,
                  fontWeight: 700,
                  fontFamily: MONO,
                  color: C.red,
                }}
              >
                {regressions}
              </span>
            </div>
          )}

          {improvements > 0 && (
            <div style={{ display: "flex", flexDirection: "column", gap: 1 }}>
              <span
                style={{
                  fontSize: 9,
                  color: C.textDim,
                  textTransform: "uppercase",
                  letterSpacing: 0.8,
                  fontWeight: 600,
                }}
              >
                Improvements
              </span>
              <span
                style={{
                  fontSize: 18,
                  fontWeight: 700,
                  fontFamily: MONO,
                  color: C.green,
                }}
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
          commit <span style={{ color: C.red }}>{sha}</span> not found
        </span>
        <Link
          to={toProjectHome}
          style={{
            color: C.accent,
            fontSize: 11,
            fontFamily: MONO,
            textDecoration: "none",
          }}
        >
          ← back to scenarios
        </Link>
      </div>
    );
  }

  return (
    <div
      style={{
        flex: 1,
        display: "flex",
        flexDirection: "column",
        overflow: "hidden",
      }}
    >
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
          onClick={() => navigate(toProjectHome)}
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
            padding: "2px 0",
          }}
          onMouseEnter={(e) => (e.currentTarget.style.color = C.accent)}
          onMouseLeave={(e) => (e.currentTarget.style.color = C.textMid)}
        >
          <ArrowLeft size={12} /> scenarios
        </button>

        <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
          {prevCommit && (
            <button
              onClick={() => navigate(toCommit(prevCommit.shortSha))}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 3,
                background: C.surface2,
                border: `1px solid ${C.border}`,
                borderRadius: 3,
                padding: "2px 8px",
                cursor: "pointer",
                color: C.textMid,
                fontSize: 10,
                fontFamily: MONO,
              }}
            >
              <ChevronLeft size={11} /> {prevCommit.shortSha}
            </button>
          )}
          {nextCommit && (
            <button
              onClick={() => navigate(toCommit(nextCommit.shortSha))}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 3,
                background: C.surface2,
                border: `1px solid ${C.border}`,
                borderRadius: 3,
                padding: "2px 8px",
                cursor: "pointer",
                color: C.textMid,
                fontSize: 10,
                fontFamily: MONO,
              }}
            >
              {nextCommit.shortSha} <ChevronRight size={11} />
            </button>
          )}
        </div>
      </div>

      <div style={{ flex: 1, overflowY: "auto", padding: 16 }}>
        <div
          style={{
            maxWidth: 860,
            margin: "0 auto",
            display: "flex",
            flexDirection: "column",
            gap: 10,
          }}
        >
          <div
            style={{
              border: `1px solid ${C.border}`,
              borderRadius: 6,
              background: C.surface,
              padding: "12px 14px",
              display: "flex",
              flexDirection: "column",
              gap: 8,
            }}
          >
            <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <GitCommit size={14} color={C.accent} />
              <span
                style={{
                  fontSize: 12,
                  fontFamily: MONO,
                  color: C.accent,
                  fontWeight: 700,
                }}
              >
                {githubCommit?.shortSha ?? commit?.shortSha ?? sha}
              </span>
            </div>

            <div style={{ fontSize: 13, fontWeight: 600, color: C.text }}>
              {messageTitle}
            </div>

            {messageBody && (
              <div
                style={{
                  whiteSpace: "pre-wrap",
                  color: C.textMid,
                  fontSize: 12,
                  lineHeight: 1.4,
                }}
              >
                {messageBody}
              </div>
            )}

            {(metaAuthor || metaTime) && (
              <div style={{ fontSize: 10, fontFamily: MONO, color: C.textDim }}>
                {metaAuthor ? `by ${metaAuthor}` : ""}
                {metaAuthor && metaTime ? " · " : ""}
                {metaTime ? fmtTime(metaTime) : ""}
              </div>
            )}

            {ghError && (
              <div style={{ fontSize: 10, fontFamily: MONO, color: C.red }}>
                GitHub metadata unavailable: {ghError}
              </div>
            )}

            {ghLoading && (
              <div style={{ fontSize: 10, fontFamily: MONO, color: C.textDim }}>
                loading GitHub metadata…
              </div>
            )}

            <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
              {githubCommit?.htmlUrl && (
                <a
                  href={githubCommit.htmlUrl}
                  target="_blank"
                  rel="noreferrer"
                  style={{
                    display: "inline-flex",
                    alignItems: "center",
                    gap: 6,
                    textDecoration: "none",
                    fontSize: 10,
                    fontFamily: MONO,
                    color: C.textMid,
                    border: `1px solid ${C.border}`,
                    borderRadius: 4,
                    padding: "4px 8px",
                    background: C.surface2,
                  }}
                >
                  <ExternalLink size={11} />
                  View diff on GitHub
                </a>
              )}

              <button
                onClick={onSchedule}
                disabled={scheduling || !targetSha}
                style={{
                  display: "inline-flex",
                  alignItems: "center",
                  gap: 6,
                  fontSize: 10,
                  fontFamily: MONO,
                  color: scheduling || !targetSha ? C.textDim : C.text,
                  border: `1px solid ${C.border}`,
                  borderRadius: 4,
                  padding: "4px 8px",
                  background: C.surface2,
                  cursor: scheduling || !targetSha ? "default" : "pointer",
                }}
              >
                <Play size={11} />
                {scheduling ? "Scheduling..." : "Schedule Forager run"}
              </button>
            </div>

            {scheduleError && (
              <div style={{ fontSize: 10, fontFamily: MONO, color: C.red }}>
                failed to schedule run: {scheduleError}
              </div>
            )}
          </div>

          {commit ? (
            <div
              style={{
                border: `1px solid ${C.border}`,
                borderRadius: 6,
                overflow: "hidden",
              }}
            >
              <CommitHeader commit={commit} C={C} />

              <div
                style={{
                  display: "grid",
                  gridTemplateColumns: "18px 1fr 70px 56px 110px",
                  gap: 8,
                  padding: "6px 12px",
                  fontSize: 8,
                  fontWeight: 700,
                  color: C.textDim,
                  textTransform: "uppercase",
                  letterSpacing: 0.8,
                  borderBottom: `1px solid ${C.border}`,
                  background: C.surface2,
                }}
              >
                <span />
                <span>Measurement</span>
                <span style={{ textAlign: "right" }}>Value</span>
                <span>Unit</span>
                <span>Δ prev</span>
              </div>

              {grouped.map(([kind, measurements], gi) => (
                <div key={kind}>
                  {grouped.length > 1 && (
                    <div
                      style={{
                        padding: "5px 12px",
                        fontSize: 9,
                        fontWeight: 700,
                        fontFamily: MONO,
                        color: C.textDim,
                        textTransform: "uppercase",
                        letterSpacing: 0.8,
                        background: C.surface,
                        borderBottom: `1px solid ${C.border}`,
                        borderTop: gi > 0 ? `1px solid ${C.border}` : undefined,
                      }}
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
            <div
              style={{
                border: `1px solid ${C.border}`,
                borderRadius: 6,
                background: C.surface,
                padding: "14px 16px",
                color: C.textDim,
                fontSize: 11,
                fontFamily: MONO,
              }}
            >
              No Forager measurements yet for this commit.
            </div>
          )}
        </div>
      </div>

      <style>{`@keyframes spin { to { transform: rotate(360deg) } }`}</style>
    </div>
  );
}
