import { useCallback, useMemo, useState } from "react";
import { useKeyboardNav } from "../lib/useKeyboardNav";
import { useParams, Link, useNavigate } from "react-router-dom";
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
import { fmtUnknown, fmtTime } from "../lib/format";
import {
  type Measurement,
  type MeasurementStatus,
  type SummaryValue,
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

function MeasurementRow({ m }: { m: Measurement }) {
  const isDone = m.status === "complete" && m.value != null;

  return (
    <div className="grid grid-cols-[18px_1fr_120px] gap-[8px] px-[12px] py-[8px] items-center border-b border-[var(--c-border)] text-[11px] font-mono cursor-default">
      <StatusIcon status={m.status} />

      <div className="flex items-center gap-[6px] overflow-hidden">
        <span className="text-fg overflow-hidden text-ellipsis whitespace-nowrap">
          {m.name}
        </span>
        {m.tags &&
          Object.entries(m.tags).map(([k, v]) => (
            <Badge key={k} color={C.textDim} bg={C.surface3}>
              {k}={v}
            </Badge>
          ))}
      </div>

      <span
        className="text-right overflow-hidden text-ellipsis whitespace-nowrap"
        style={{ color: isDone ? C.textMid : C.textDim }}
      >
        {isDone ? fmtUnknown(m.value) : statusLabel(m.status)}
      </span>
    </div>
  );
}

// ── Summaries panel ──────────────────────────────────────────────────────────

function SummariesPanel({ summaries }: { summaries: SummaryValue[] }) {
  if (summaries.length === 0) return null;
  return (
    <div className="flex flex-wrap gap-[8px] px-[12px] py-[8px] border-b border-[var(--c-border)] bg-surface2">
      {summaries.map((s) => (
        <div key={s.name} className="flex flex-col gap-[1px] min-w-[80px]">
          <span className="text-[9px] font-mono text-dim uppercase tracking-[0.6px]">
            {s.name}
          </span>
          <span
            className="text-[12px] font-mono font-semibold"
            style={{ color: C.pink }}
          >
            {s.value.toLocaleString(undefined, { maximumFractionDigits: 2 })}
          </span>
        </div>
      ))}
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
  const [knownExperiments, setKnownExperiments] = useState<string[]>([]);
  const [experimentInput, setExperimentInput] = useState("");
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

  const byExperiment = useMemo(() => {
    if (!commit) return [];
    const expMap = new Map<string, Map<string, Measurement[]>>();
    for (const m of commit.measurements) {
      const exp = m.experimentName ?? "";
      const step = m.step ?? m.name;
      if (!expMap.has(exp)) expMap.set(exp, new Map());
      const stepMap = expMap.get(exp)!;
      if (!stepMap.has(step)) stepMap.set(step, []);
      stepMap.get(step)!.push(m);
    }
    return Array.from(expMap.entries()).map(([exp, stepMap]) => ({
      exp,
      steps: Array.from(stepMap.entries()),
      summaries: (commit.summaries ?? []).filter(
        (s) => s.experimentName === exp,
      ),
    }));
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
      const bms = await pApi.experiments();
      setKnownExperiments(bms);
      if (bms.length === 1) setExperimentInput(bms[0]);
    } catch {
      // Ignore — user can still type manually.
    }
  };

  const onEnqueue = async () => {
    const name = experimentInput.trim();
    if (!targetSha || !name || !current || enqueueing) return;
    setEnqueueing(true);
    setEnqueueError(null);
    try {
      await api.enqueueForagerJob(current.upstream, targetSha, name);
      setEnqueueSuccess(true);
      setShowPicker(false);
      setExperimentInput("");
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
                {knownExperiments.length > 0 && (
                  <div className="flex gap-[4px] flex-wrap">
                    {knownExperiments.map((bm) => (
                      <button
                        key={bm}
                        onClick={() => setExperimentInput(bm)}
                        className="text-[10px] font-mono border border-[var(--c-border)] rounded px-[6px] py-[4px]"
                        style={{
                          background:
                            experimentInput === bm ? C.surface3 : C.surface2,
                          color: experimentInput === bm ? C.accent : C.textMid,
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
                    value={experimentInput}
                    onChange={(e) => setExperimentInput(e.target.value)}
                    onKeyDown={(e) => e.key === "Enter" && onEnqueue()}
                    placeholder="experiment name"
                    className="text-[10px] font-mono border border-[var(--c-border)] rounded px-[6px] py-[5px] bg-surface2 text-fg flex-1"
                    style={{ outline: "none" }}
                    autoFocus
                  />
                  <button
                    onClick={onEnqueue}
                    disabled={!experimentInput.trim() || enqueueing}
                    className="text-[10px] font-mono border border-[var(--c-border)] rounded px-[8px] py-[5px] bg-surface2"
                    style={{
                      color:
                        !experimentInput.trim() || enqueueing
                          ? C.textDim
                          : C.accent,
                      cursor:
                        !experimentInput.trim() || enqueueing
                          ? "default"
                          : "pointer",
                    }}
                  >
                    {enqueueing ? "Enqueueing…" : "Enqueue"}
                  </button>
                  <button
                    onClick={() => {
                      setShowPicker(false);
                      setExperimentInput("");
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

          {commit && byExperiment.length > 0 ? (
            byExperiment.map(({ exp, steps, summaries }) => (
              <div
                key={exp}
                className="border border-[var(--c-border)] rounded-md overflow-hidden"
              >
                <div className="px-[12px] py-[8px] bg-surface2 border-b border-[var(--c-border)] flex items-center gap-[6px]">
                  <span className="text-[10px] font-bold font-mono text-dim uppercase tracking-[0.8px]">
                    experiment
                  </span>
                  <span className="text-[11px] font-mono font-semibold text-accent">
                    {exp || "(default)"}
                  </span>
                </div>

                <SummariesPanel summaries={summaries} />

                <div className="grid grid-cols-[18px_1fr_120px] gap-[8px] px-[12px] py-[6px] text-[10px] font-bold text-dim uppercase tracking-[0.8px] border-b border-[var(--c-border)] bg-surface">
                  <span />
                  <span>Measurement</span>
                  <span className="text-right">Value</span>
                </div>

                {steps.map(([stepName, measurements]) => {
                  const summarySpec = vizMap[stepName]?.summary;
                  const completedMs = measurements.filter(
                    (m) => m.status === "complete" && m.value != null,
                  );
                  return (
                    <div key={stepName}>
                      {steps.length > 1 && (
                        <div className="px-[12px] py-[5px] text-[10px] font-bold font-mono text-mid bg-surface border-b border-[var(--c-border)]">
                          {stepName}
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
                          />
                        </div>
                      )}
                      {measurements.map((m) => (
                        <MeasurementRow key={m.id} m={m} />
                      ))}
                    </div>
                  );
                })}
              </div>
            ))
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
