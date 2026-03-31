import { useCallback, useMemo, useState } from "react";
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
import { C } from "../lib/colors";
import { fmtValue, fmtTime } from "../lib/format";
import {
  type ForagerCommit,
  type Measurement,
  type MeasurementStatus,
  buildVizMap,
} from "../lib/data";
import { useCommits, useGithubCommit, usePheromones } from "../lib/hooks";
import { useProject } from "../lib/useProject";
import { api } from "../lib/api";
import { Badge } from "../components/Badge";
import { VizRenderer } from "../components/VizRenderer";

// ── Small pieces ─────────────────────────────────────────────────────────────

function StatusIcon({ status }: { status: MeasurementStatus }) {
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

// ── Measurement row ──────────────────────────────────────────────────────────

function MeasurementRow({
  m,
  projectId,
  commitSha,
  navigate,
}: {
  m: Measurement;
  projectId: number;
  commitSha: string;
  navigate: NavigateFunction;
}) {
  const [hovered, setHovered] = useState(false);
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
      <StatusIcon status={m.status} />

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

      <span className="text-dim text-[10px]">
        {isDone && m.unit ? m.unit : ""}
      </span>

      <span className="text-dim text-[10px]">—</span>
    </div>
  );
}

// ── Commit header ────────────────────────────────────────────────────────────

function CommitHeader({ commit }: { commit: ForagerCommit }) {
  const completedMs = commit.measurements.filter(
    (m) => m.status === "complete" && m.value != null && m.unit === "ms",
  );

  const totalMs =
    completedMs.length > 0
      ? completedMs.reduce((s, m) => s + (m.value ?? 0), 0)
      : null;

  return (
    <div className="flex flex-col gap-[12px] px-[20px] py-[16px] bg-surface border-b border-[var(--c-border)] rounded-t-md">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-[8px]">
          <GitCommit size={16} color={C.accent} />
          <span className="text-sm font-bold font-mono text-accent tracking-[-0.3px]">
            {commit.shortSha}
          </span>
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

      <div className="flex gap-[20px] items-end flex-wrap">
        {totalMs != null && (
          <div className="flex flex-col gap-[1px]">
            <span className="text-[10px] text-dim uppercase tracking-[0.8px] font-semibold">
              Σ timed measurements
            </span>
            <span className="text-lg font-bold font-mono text-fg">
              {fmtValue(totalMs, "ms")}
            </span>
          </div>
        )}

        <div className="flex flex-col gap-[1px]">
          <span className="text-[10px] text-dim uppercase tracking-[0.8px] font-semibold">
            Measurements
          </span>
          <span
            className="text-lg font-bold font-mono"
            style={{ color: C.pink }}
          >
            {commit.measurements.length}
          </span>
        </div>
      </div>
    </div>
  );
}

// ── Page ─────────────────────────────────────────────────────────────────────

export default function CommitPage() {
  const { projectId: projectIdParam, sha } = useParams();
  const projectId = Number(projectIdParam);
  const hasProjectId = Number.isFinite(projectId);

  const navigate = useNavigate();
  const { pApi, current } = useProject();
  const { commits } = useCommits();
  const { pheromones } = usePheromones();
  const vizMap = useMemo(() => buildVizMap(pheromones), [pheromones]);

  const [showPicker, setShowPicker] = useState(false);
  const [knownBenchmarks, setKnownBenchmarks] = useState<string[]>([]);
  const [benchmarkInput, setBenchmarkInput] = useState("");
  const [enqueueing, setEnqueueing] = useState(false);
  const [enqueueError, setEnqueueError] = useState<string | null>(null);
  const [enqueueSuccess, setEnqueueSuccess] = useState(false);

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
  const toCommit = useCallback(
    (s: string) =>
      hasProjectId ? `/project/${projectId}/commit/${s}` : `/commit/${s}`,
    [hasProjectId, projectId],
  );

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

  const onClickSchedule = async () => {
    if (!targetSha || showPicker) return;
    setEnqueueError(null);
    setEnqueueSuccess(false);
    setShowPicker(true);
    try {
      const bms = await pApi.benchmarks();
      setKnownBenchmarks(bms);
      if (bms.length === 1) setBenchmarkInput(bms[0]);
    } catch {
      // Ignore — user can still type manually.
    }
  };

  const onEnqueue = async () => {
    const name = benchmarkInput.trim();
    if (!targetSha || !name || !current || enqueueing) return;
    setEnqueueing(true);
    setEnqueueError(null);
    try {
      await api.enqueueForagerJob(current.upstream, targetSha, name);
      setEnqueueSuccess(true);
      setShowPicker(false);
      setBenchmarkInput("");
    } catch (e) {
      setEnqueueError(String(e));
    } finally {
      setEnqueueing(false);
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
          ← back to observations
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
          <ArrowLeft size={12} /> observations
        </button>

        <div className="flex items-center gap-[6px]">
          {prevCommit && (
            <button
              onClick={() => navigate(toCommit(prevCommit.shortSha))}
              className="flex items-center gap-[3px] bg-surface2 border border-[var(--c-border)] rounded-[3px] px-[8px] py-[4px] cursor-pointer text-mid text-[10px] font-mono"
            >
              <ChevronLeft size={11} /> {prevCommit.shortSha}
            </button>
          )}
          {nextCommit && (
            <button
              onClick={() => navigate(toCommit(nextCommit.shortSha))}
              className="flex items-center gap-[3px] bg-surface2 border border-[var(--c-border)] rounded-[3px] px-[8px] py-[4px] cursor-pointer text-mid text-[10px] font-mono"
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
                onClick={onClickSchedule}
                disabled={!targetSha || showPicker}
                className="inline-flex items-center gap-[6px] text-[10px] font-mono border border-[var(--c-border)] rounded px-[8px] py-[4px] bg-surface2"
                style={{
                  color: !targetSha || showPicker ? C.textDim : C.text,
                  cursor: !targetSha || showPicker ? "default" : "pointer",
                }}
              >
                <Play size={11} />
                {enqueueSuccess ? "Queued!" : "Schedule Forager run"}
              </button>
            </div>

            {showPicker && (
              <div className="flex flex-col gap-[6px] border-t border-[var(--c-border)] pt-[8px]">
                {knownBenchmarks.length > 0 && (
                  <div className="flex gap-[4px] flex-wrap">
                    {knownBenchmarks.map((bm) => (
                      <button
                        key={bm}
                        onClick={() => setBenchmarkInput(bm)}
                        className="text-[10px] font-mono border border-[var(--c-border)] rounded px-[6px] py-[4px]"
                        style={{
                          background:
                            benchmarkInput === bm ? C.surface3 : C.surface2,
                          color: benchmarkInput === bm ? C.accent : C.textMid,
                          cursor: "pointer",
                        }}
                      >
                        {bm}
                      </button>
                    ))}
                  </div>
                )}
                <div className="flex items-center gap-[6px]">
                  <input
                    value={benchmarkInput}
                    onChange={(e) => setBenchmarkInput(e.target.value)}
                    onKeyDown={(e) => e.key === "Enter" && onEnqueue()}
                    placeholder="benchmark name"
                    className="text-[10px] font-mono border border-[var(--c-border)] rounded px-[6px] py-[5px] bg-surface2 text-fg flex-1"
                    style={{ outline: "none" }}
                    autoFocus
                  />
                  <button
                    onClick={onEnqueue}
                    disabled={!benchmarkInput.trim() || enqueueing}
                    className="text-[10px] font-mono border border-[var(--c-border)] rounded px-[8px] py-[5px] bg-surface2"
                    style={{
                      color:
                        !benchmarkInput.trim() || enqueueing
                          ? C.textDim
                          : C.accent,
                      cursor:
                        !benchmarkInput.trim() || enqueueing
                          ? "default"
                          : "pointer",
                    }}
                  >
                    {enqueueing ? "Enqueueing…" : "Enqueue"}
                  </button>
                  <button
                    onClick={() => {
                      setShowPicker(false);
                      setBenchmarkInput("");
                      setEnqueueError(null);
                    }}
                    className="text-[10px] font-mono text-dim border-0 bg-transparent cursor-pointer"
                  >
                    cancel
                  </button>
                </div>
                {enqueueError && (
                  <div
                    className="text-[10px] font-mono"
                    style={{ color: C.red }}
                  >
                    {enqueueError}
                  </div>
                )}
              </div>
            )}
          </div>

          {commit ? (
            <div className="border border-[var(--c-border)] rounded-md overflow-hidden">
              <CommitHeader commit={commit} />

              <div className="grid grid-cols-[18px_1fr_70px_56px_110px] gap-[8px] px-[12px] py-[8px] text-[10px] font-bold text-dim uppercase tracking-[0.8px] border-b border-[var(--c-border)] bg-surface2">
                <span />
                <span>Measurement</span>
                <span className="text-right">Value</span>
                <span>Unit</span>
                <span>Δ prev</span>
              </div>

              {grouped.map(([kind, measurements], gi) => {
                const summarySpec = vizMap[kind]?.summary;
                const completedMs = measurements.filter(
                  (m) => m.status === "complete" && m.value != null,
                );
                return (
                  <div key={kind}>
                    {grouped.length > 1 && (
                      <div
                        className={`px-[12px] py-[6px] text-[10px] font-bold font-mono text-dim uppercase tracking-[0.8px] bg-surface border-b border-[var(--c-border)]${gi > 0 ? " border-t border-[var(--c-border)]" : ""}`}
                      >
                        {kind}
                      </div>
                    )}
                    {summarySpec && completedMs.length > 0 && (
                      <div className="px-[12px] py-[8px] border-b border-[var(--c-border)] bg-surface">
                        <VizRenderer
                          spec={summarySpec}
                          data={completedMs.map((m) => ({
                            name: m.name,
                            value: m.value,
                          }))}
                          unit={completedMs[0]?.unit}
                        />
                      </div>
                    )}
                    {measurements.map((m) => (
                      <MeasurementRow
                        key={m.id}
                        m={m}
                        projectId={projectId}
                        commitSha={commit.shortSha}
                        navigate={navigate}
                      />
                    ))}
                  </div>
                );
              })}
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
