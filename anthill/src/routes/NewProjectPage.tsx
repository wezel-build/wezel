import { useState, useEffect, type FormEvent } from "react";
import { useNavigate } from "react-router-dom";
import { C } from "../lib/colors";
import { useProject } from "../lib/useProject";
import { api, type GithubRepoEntry } from "../lib/api";
import type { Repo } from "../lib/data";
import {
  Workflow,
  Check,
  ChevronRight,
  ChevronLeft,
  FolderGit2,
  Plus,
  Lock,
} from "lucide-react";

// ── Shared styles ──────────────────────────────────────────────────────────

const inputClass =
  "w-full px-[10px] py-[8px] text-[13px] font-mono bg-surface text-fg border border-[var(--c-border)] rounded-md outline-none";

const labelClass =
  "block text-[10px] font-mono font-bold text-dim uppercase tracking-[0.8px] mb-[4px]";

function Btn({
  children,
  disabled,
  onClick,
  type = "button",
  variant = "primary",
}: {
  children: React.ReactNode;
  disabled?: boolean;
  onClick?: () => void;
  type?: "button" | "submit";
  variant?: "primary" | "secondary";
}) {
  const primary = variant === "primary";
  return (
    <button
      type={type}
      disabled={disabled}
      onClick={onClick}
      className={`text-[11px] font-mono font-bold px-[20px] py-[7px] border-0 rounded-md flex items-center gap-[6px] ${disabled ? "cursor-not-allowed" : "cursor-pointer"}`}
      style={{
        background: primary
          ? disabled
            ? C.surface2
            : C.accent
          : "transparent",
        color: primary ? (disabled ? C.textDim : C.bg) : C.textDim,
        border: primary ? "none" : `1px solid var(--c-border)`,
      }}
    >
      {children}
    </button>
  );
}

// ── Step 1: Repository ──────────────────────────────────────────────────────

function StepRepo({
  existingRepos,
  githubRepos,
  loadingGithub,
  selected,
  onSelect,
  onNext,
}: {
  existingRepos: Repo[];
  githubRepos: GithubRepoEntry[];
  loadingGithub: boolean;
  selected: string;
  onSelect: (upstream: string) => void;
  onNext: () => void;
}) {
  const canNext = selected !== "";

  // Merge: show existing repos + github repos not already tracked.
  const existingUpstreams = new Set(existingRepos.map((r) => r.upstream));
  const untracked = githubRepos.filter(
    (g) =>
      !existingUpstreams.has(`github.com/${g.full_name}`) &&
      !existingUpstreams.has(g.full_name),
  );

  return (
    <>
      <h2 className="text-sm font-mono font-bold text-fg m-0 mb-[4px]">
        Repository
      </h2>
      <p className="text-[11px] font-mono text-dim m-0 mb-[16px]">
        Select a repository to create a project for.
      </p>

      {/* Existing repos already in wezel */}
      {existingRepos.length > 0 && (
        <div className="mb-[14px] flex flex-col gap-[4px]">
          <div className="text-[10px] font-mono font-bold text-dim uppercase tracking-[0.8px] mb-[2px]">
            Tracked repos
          </div>
          {existingRepos.map((r) => {
            const active = selected === r.upstream;
            return (
              <button
                key={r.id}
                type="button"
                onClick={() => onSelect(r.upstream)}
                className="flex items-center gap-[8px] w-full text-left bg-transparent border rounded-md cursor-pointer py-[8px] px-[10px] font-mono text-[12px]"
                style={{
                  borderColor: active ? "var(--c-accent)" : "var(--c-border)",
                  background: active ? "var(--c-surface2)" : "transparent",
                  color: "var(--c-text)",
                }}
              >
                <FolderGit2
                  size={13}
                  color={active ? "var(--c-accent)" : "var(--c-text-dim)"}
                />
                <div className="flex-1 min-w-0">
                  <div className="truncate">{r.upstream}</div>
                  <div className="text-[10px] text-dim mt-[1px]">
                    {r.projectCount} project{r.projectCount !== 1 ? "s" : ""}
                  </div>
                </div>
                {active && <Check size={14} color="var(--c-accent)" />}
              </button>
            );
          })}
        </div>
      )}

      {/* GitHub repos from app installations */}
      {loadingGithub ? (
        <div className="text-[11px] font-mono text-dim py-[12px]">
          Loading repositories from GitHub...
        </div>
      ) : untracked.length > 0 ? (
        <div className="mb-[14px] flex flex-col gap-[4px]">
          <div className="text-[10px] font-mono font-bold text-dim uppercase tracking-[0.8px] mb-[2px]">
            Available from GitHub
          </div>
          {untracked.map((g) => {
            const upstream = `github.com/${g.full_name}`;
            const active = selected === upstream;
            return (
              <button
                key={g.full_name}
                type="button"
                onClick={() => onSelect(upstream)}
                className="flex items-center gap-[8px] w-full text-left bg-transparent border rounded-md cursor-pointer py-[8px] px-[10px] font-mono text-[12px]"
                style={{
                  borderColor: active ? "var(--c-accent)" : "var(--c-border)",
                  background: active ? "var(--c-surface2)" : "transparent",
                  color: "var(--c-text)",
                }}
              >
                <FolderGit2
                  size={13}
                  color={active ? "var(--c-accent)" : "var(--c-text-dim)"}
                />
                <div className="flex-1 min-w-0">
                  <div className="truncate flex items-center gap-[4px]">
                    {g.full_name}
                    {g.private && (
                      <Lock size={10} className="text-dim shrink-0" />
                    )}
                  </div>
                </div>
                {active && <Check size={14} color="var(--c-accent)" />}
              </button>
            );
          })}
        </div>
      ) : (
        !loadingGithub &&
        githubRepos.length === 0 && (
          <div className="text-[11px] font-mono text-dim py-[8px] mb-[14px]">
            No GitHub repos found. Install the Wezel app on your org to see
            repos here.
          </div>
        )
      )}

      {/* Manual URL fallback */}
      <details className="mb-[4px]">
        <summary className="text-[10px] font-mono text-dim cursor-pointer mb-[6px]">
          <Plus size={10} className="inline mr-[4px] align-[-1px]" />
          Enter URL manually
        </summary>
        <input
          placeholder="https://github.com/org/repo"
          value={
            existingRepos.some((r) => r.upstream === selected) ||
            githubRepos.some((g) => `github.com/${g.full_name}` === selected)
              ? ""
              : selected
          }
          onChange={(e) => onSelect(e.target.value.trim())}
          className={inputClass}
        />
      </details>

      <div className="flex justify-end mt-[20px]">
        <Btn disabled={!canNext} onClick={onNext}>
          Next <ChevronRight size={12} />
        </Btn>
      </div>
    </>
  );
}

// ── Step 2: Project ─────────────────────────────────────────────────────────

function StepProject({
  repoUpstream,
  name,
  onName,
  creating,
  error,
  onBack,
  onSubmit,
}: {
  repoUpstream: string;
  name: string;
  onName: (n: string) => void;
  creating: boolean;
  error: string | null;
  onBack: () => void;
  onSubmit: () => void;
}) {
  const canSubmit = name.trim() && !creating;

  const suggestName = () => {
    const parts = repoUpstream.replace(/\.git$/, "").split("/");
    return parts[parts.length - 1] || "";
  };

  useEffect(() => {
    if (!name) onName(suggestName());
  }, [repoUpstream]); // eslint-disable-line react-hooks/exhaustive-deps

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault();
    if (canSubmit) onSubmit();
  };

  return (
    <form onSubmit={handleSubmit}>
      <h2 className="text-sm font-mono font-bold text-fg m-0 mb-[4px]">
        Project
      </h2>
      <p className="text-[11px] font-mono text-dim m-0 mb-[16px]">
        Create a project for <span className="text-mid">{repoUpstream}</span>
      </p>

      <label className={labelClass}>Name</label>
      <input
        autoFocus
        placeholder="my-project"
        value={name}
        onChange={(e) => {
          onName(e.target.value);
        }}
        className={`${inputClass} mb-[4px]`}
        style={error ? { borderColor: "var(--c-red)" } : undefined}
      />
      {error ? (
        <div className="text-[11px] font-mono text-c-red mb-[14px]">
          {error}
        </div>
      ) : (
        <div className="mb-[14px]" />
      )}

      <div className="bg-surface2 border border-[var(--c-border)] rounded-md px-[12px] py-[10px] mb-[20px]">
        <div className="text-[10px] font-mono font-bold text-dim uppercase tracking-[0.8px] mb-[6px]">
          Project location
        </div>
        <p className="text-[11px] font-mono text-mid m-0 mb-[4px]">
          Add a <code className="text-accent">.wezel/config.toml</code> to the
          root of your repository:
        </p>
        <pre className="bg-bg rounded px-[10px] py-[6px] text-[11px] font-mono text-fg m-0 overflow-x-auto">
          {`[project]\nburrow_url = "${window.location.origin}"`}
        </pre>
        <p className="text-[10px] font-mono text-dim m-0 mt-[6px]">
          Experiments go in{" "}
          <code className="text-mid">.wezel/benchmarks/&lt;name&gt;/</code>
        </p>
      </div>

      <div className="flex gap-[8px] justify-between">
        <Btn variant="secondary" onClick={onBack}>
          <ChevronLeft size={12} /> Back
        </Btn>
        <Btn type="submit" disabled={!canSubmit}>
          {creating ? "creating..." : "create project"}
        </Btn>
      </div>
    </form>
  );
}

// ── Main page ───────────────────────────────────────────────────────────────

export default function NewProjectPage() {
  const navigate = useNavigate();
  const { projects, addProject } = useProject();

  const [step, setStep] = useState<1 | 2>(1);
  const [repos, setRepos] = useState<Repo[]>([]);
  const [githubRepos, setGithubRepos] = useState<GithubRepoEntry[]>([]);
  const [loadingGithub, setLoadingGithub] = useState(true);

  // Step 1 state
  const [selected, setSelected] = useState("");

  // Step 2 state
  const [name, setName] = useState("");
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api.repos().then(setRepos).catch(console.error);
    api
      .githubRepos()
      .then(setGithubRepos)
      .catch(console.error)
      .finally(() => setLoadingGithub(false));
  }, []);

  const handleCreate = async () => {
    if (!name.trim() || !selected) return;
    setCreating(true);
    setError(null);
    try {
      await addProject(name.trim(), selected);
      navigate("/");
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setCreating(false);
    }
  };

  const empty = projects.length === 0;

  const steps = [
    { n: 1, label: "repo" },
    { n: 2, label: "project" },
  ] as const;

  return (
    <div className="flex-1 flex items-center justify-center p-[24px]">
      <div className="w-full max-w-[460px]">
        {empty && step === 1 && (
          <div className="flex items-center gap-[8px] mb-[24px] justify-center">
            <Workflow size={20} color={C.accent} strokeWidth={2.5} />
            <span className="text-base font-extrabold text-accent tracking-[-0.5px] font-sans">
              Welcome to wezel
            </span>
          </div>
        )}

        {/* Step indicator */}
        <div className="flex items-center gap-[4px] mb-[20px]">
          {steps.map(({ n, label }, i) => (
            <div key={n} className="flex items-center gap-[4px]">
              {i > 0 && (
                <div
                  className="w-[20px] h-[1px]"
                  style={{
                    background:
                      step >= n ? "var(--c-accent)" : "var(--c-border)",
                  }}
                />
              )}
              <div
                className="flex items-center gap-[4px]"
                style={{
                  opacity: step >= n ? 1 : 0.4,
                }}
              >
                <div
                  className="w-[18px] h-[18px] rounded-full flex items-center justify-center text-[10px] font-mono font-bold"
                  style={{
                    background:
                      step > n
                        ? "var(--c-green)"
                        : step === n
                          ? "var(--c-accent)"
                          : "var(--c-surface2)",
                    color: step >= n ? "var(--c-bg)" : "var(--c-text-dim)",
                  }}
                >
                  {step > n ? <Check size={10} /> : n}
                </div>
                <span className="text-[10px] font-mono text-dim">{label}</span>
              </div>
            </div>
          ))}
        </div>

        {step === 1 && (
          <StepRepo
            existingRepos={repos}
            githubRepos={githubRepos}
            loadingGithub={loadingGithub}
            selected={selected}
            onSelect={setSelected}
            onNext={() => setStep(2)}
          />
        )}

        {step === 2 && (
          <StepProject
            repoUpstream={selected}
            name={name}
            onName={setName}
            creating={creating}
            error={error}
            onBack={() => setStep(1)}
            onSubmit={handleCreate}
          />
        )}

        {/* Cancel link for non-empty state */}
        {!empty && (
          <div className="text-center mt-[16px]">
            <button
              type="button"
              onClick={() => navigate("/")}
              className="text-[10px] font-mono text-dim bg-transparent border-none cursor-pointer underline"
            >
              cancel
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
