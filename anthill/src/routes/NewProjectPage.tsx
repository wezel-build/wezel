import { useState, useEffect, type FormEvent } from "react";
import { useNavigate } from "react-router-dom";
import { C } from "../lib/colors";
import { useProject } from "../lib/useProject";
import { api } from "../lib/api";
import type { Repo } from "../lib/data";
import {
  Workflow,
  Check,
  Copy,
  ExternalLink,
  ChevronRight,
  ChevronLeft,
  ShieldCheck,
  FolderGit2,
  Plus,
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
  repos,
  selectedRepoId,
  newUrl,
  onSelect,
  onNewUrl,
  onNext,
}: {
  repos: Repo[];
  selectedRepoId: number | null;
  newUrl: string;
  onSelect: (id: number | null) => void;
  onNewUrl: (url: string) => void;
  onNext: () => void;
}) {
  const addingNew = selectedRepoId === null && newUrl !== "";
  const canNext = selectedRepoId !== null || newUrl.trim() !== "";

  return (
    <>
      <h2 className="text-sm font-mono font-bold text-fg m-0 mb-[4px]">
        Repository
      </h2>
      <p className="text-[11px] font-mono text-dim m-0 mb-[16px]">
        Select an existing repo or add a new one.
      </p>

      {repos.length > 0 && (
        <div className="mb-[14px] flex flex-col gap-[4px]">
          {repos.map((r) => (
            <button
              key={r.id}
              type="button"
              onClick={() => {
                onSelect(r.id);
                onNewUrl("");
              }}
              className="flex items-center gap-[8px] w-full text-left bg-transparent border rounded-md cursor-pointer py-[8px] px-[10px] font-mono text-[12px]"
              style={{
                borderColor:
                  selectedRepoId === r.id
                    ? "var(--c-accent)"
                    : "var(--c-border)",
                background:
                  selectedRepoId === r.id ? "var(--c-surface2)" : "transparent",
                color: "var(--c-text)",
              }}
            >
              <FolderGit2
                size={13}
                color={
                  selectedRepoId === r.id
                    ? "var(--c-accent)"
                    : "var(--c-text-dim)"
                }
              />
              <div className="flex-1 min-w-0">
                <div className="truncate">{r.upstream}</div>
                <div className="text-[10px] text-dim mt-[1px]">
                  {r.projectCount} project{r.projectCount !== 1 ? "s" : ""}
                  {r.webhookRegistered && (
                    <span className="ml-[8px] text-c-green">
                      <ShieldCheck
                        size={10}
                        className="inline mr-[2px] align-[-1px]"
                      />
                      webhook
                    </span>
                  )}
                </div>
              </div>
              {selectedRepoId === r.id && (
                <Check size={14} color="var(--c-accent)" />
              )}
            </button>
          ))}
        </div>
      )}

      <div
        className="border rounded-md px-[10px] py-[8px]"
        style={{
          borderColor: addingNew ? "var(--c-accent)" : "var(--c-border)",
          background: addingNew ? "var(--c-surface2)" : "transparent",
        }}
      >
        <div className="flex items-center gap-[6px] mb-[6px]">
          <Plus size={12} color="var(--c-text-dim)" />
          <span className="text-[10px] font-mono font-bold text-dim uppercase tracking-[0.8px]">
            New repository
          </span>
        </div>
        <input
          placeholder="https://github.com/org/repo"
          value={newUrl}
          onChange={(e) => {
            onNewUrl(e.target.value);
            if (e.target.value) onSelect(null);
          }}
          onFocus={() => {
            if (newUrl) onSelect(null);
          }}
          className={inputClass}
        />
      </div>

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

  // Suggest a name from the repo URL.
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

// ── Step 3: Webhook ─────────────────────────────────────────────────────────

function StepWebhook({
  repoId,
  repoUpstream,
  webhookRegistered,
  onDone,
}: {
  repoId: number;
  repoUpstream: string;
  webhookRegistered: boolean;
  onDone: () => void;
}) {
  const [result, setResult] = useState<{
    registered: boolean;
    webhookUrl: string;
    secret: string;
  } | null>(null);
  const [setting, setSetting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState<"url" | "secret" | null>(null);

  const handleSetup = async () => {
    setSetting(true);
    setError(null);
    try {
      const res = await api.setupWebhook(repoId);
      setResult({
        registered: res.registered,
        webhookUrl: res.webhookUrl,
        secret: res.webhookSecret,
      });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSetting(false);
    }
  };

  const copyToClipboard = async (text: string, label: "url" | "secret") => {
    await navigator.clipboard.writeText(text);
    setCopied(label);
    setTimeout(() => setCopied(null), 2000);
  };

  return (
    <>
      <h2 className="text-sm font-mono font-bold text-fg m-0 mb-[4px]">
        Webhook setup
      </h2>
      <p className="text-[11px] font-mono text-dim m-0 mb-[16px]">
        Connect a GitHub webhook so wezel tracks commits automatically.
      </p>

      {!result ? (
        <>
          {webhookRegistered && (
            <p className="text-[11px] font-mono text-dim m-0 mb-[12px]">
              <ShieldCheck
                size={11}
                className="inline mr-[3px] align-[-1px] text-c-green"
              />
              A webhook is already configured. Setting up again will replace it.
            </p>
          )}

          <div className="flex gap-[8px] items-center mb-[20px]">
            <Btn onClick={handleSetup} disabled={setting}>
              <ShieldCheck size={12} />
              {setting ? "setting up..." : "set up webhook"}
            </Btn>
            <span className="text-[10px] font-mono text-dim">
              Registers the webhook on GitHub automatically
            </span>
          </div>

          {error && (
            <div className="text-[11px] font-mono text-c-red mb-[12px]">
              {error}
            </div>
          )}

          <div className="flex gap-[8px] justify-between">
            <div />
            <Btn variant="secondary" onClick={onDone}>
              skip for now <ChevronRight size={12} />
            </Btn>
          </div>
        </>
      ) : result.registered ? (
        /* Auto-registered successfully */
        <>
          <div className="bg-surface2 border border-[var(--c-border)] rounded-md px-[12px] py-[10px] mb-[20px]">
            <div className="flex items-center gap-[6px] mb-[4px]">
              <Check size={14} color="var(--c-green)" />
              <span className="text-[12px] font-mono font-bold text-c-green">
                Webhook registered
              </span>
            </div>
            <p className="text-[11px] font-mono text-dim m-0">
              Push events from this repo will now be tracked automatically.
            </p>
          </div>

          <div className="flex justify-end">
            <Btn onClick={onDone}>
              Done <ChevronRight size={12} />
            </Btn>
          </div>
        </>
      ) : (
        /* Fallback: manual setup */
        <>
          <div className="bg-surface2 border border-[var(--c-border)] rounded-md px-[12px] py-[8px] mb-[14px] text-[11px] font-mono text-dim">
            Automatic registration wasn't possible (missing or insufficient
            GitHub token). Set it up manually:
          </div>

          <label className={labelClass}>Payload URL</label>
          <div className="flex gap-[4px] mb-[14px]">
            <div className={`${inputClass} flex-1 select-all`}>
              {result.webhookUrl}
            </div>
            <button
              type="button"
              onClick={() => copyToClipboard(result.webhookUrl, "url")}
              className="bg-surface2 border border-[var(--c-border)] rounded-md px-[8px] cursor-pointer text-dim flex items-center"
              title="Copy"
            >
              {copied === "url" ? (
                <Check size={13} color="var(--c-green)" />
              ) : (
                <Copy size={13} />
              )}
            </button>
          </div>

          <label className={labelClass}>Secret</label>
          <div className="flex gap-[4px] mb-[14px]">
            <div
              className={`${inputClass} flex-1 text-c-green select-all break-all`}
            >
              {result.secret}
            </div>
            <button
              type="button"
              onClick={() => copyToClipboard(result.secret, "secret")}
              className="bg-surface2 border border-[var(--c-border)] rounded-md px-[8px] cursor-pointer text-dim flex items-center"
              title="Copy"
            >
              {copied === "secret" ? (
                <Check size={13} color="var(--c-green)" />
              ) : (
                <Copy size={13} />
              )}
            </button>
          </div>

          <div className="bg-surface2 border border-[var(--c-border)] rounded-md px-[12px] py-[10px] mb-[20px]">
            <div className="text-[10px] font-mono font-bold text-dim uppercase tracking-[0.8px] mb-[6px]">
              GitHub setup
            </div>
            <ol className="text-[11px] font-mono text-mid m-0 pl-[16px] flex flex-col gap-[4px]">
              <li>
                <a
                  href={`${repoUpstream.replace(/\.git$/, "")}/settings/hooks/new`}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-accent no-underline hover:underline"
                >
                  Add a webhook
                  <ExternalLink
                    size={10}
                    className="inline ml-[3px] align-[-1px]"
                  />
                </a>{" "}
                in your repo settings
              </li>
              <li>
                Set <strong>Payload URL</strong> and <strong>Secret</strong> to
                the values above
              </li>
              <li>
                Set <strong>Content type</strong> to{" "}
                <code className="text-accent">application/json</code>
              </li>
              <li>
                Select <strong>Just the push event</strong>
              </li>
            </ol>
          </div>

          <div className="bg-surface2 border border-[var(--c-border)] rounded-md px-[12px] py-[8px] mb-[20px] text-[10px] font-mono text-dim">
            This secret is shown only once. Copy it now.
          </div>

          <div className="flex justify-end">
            <Btn onClick={onDone}>
              Done <ChevronRight size={12} />
            </Btn>
          </div>
        </>
      )}
    </>
  );
}

// ── Main page ───────────────────────────────────────────────────────────────

export default function NewProjectPage() {
  const navigate = useNavigate();
  const { projects, addProject } = useProject();

  const [step, setStep] = useState<1 | 2 | 3>(1);
  const [repos, setRepos] = useState<Repo[]>([]);

  // Step 1 state
  const [selectedRepoId, setSelectedRepoId] = useState<number | null>(null);
  const [newUrl, setNewUrl] = useState("");

  // Step 2 state
  const [name, setName] = useState("");
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Step 3 state
  const [createdRepoId, setCreatedRepoId] = useState<number | null>(null);
  const [createdRepoWebhookRegistered, setCreatedRepoHasSecret] =
    useState(false);

  useEffect(() => {
    api.repos().then(setRepos).catch(console.error);
  }, []);

  const upstream =
    selectedRepoId !== null
      ? (repos.find((r) => r.id === selectedRepoId)?.upstream ?? "")
      : newUrl.trim();

  const handleCreate = async () => {
    if (!name.trim() || !upstream) return;
    setCreating(true);
    setError(null);
    try {
      await addProject(name.trim(), upstream);
      // Determine repo info for webhook step.
      const selectedRepo = repos.find((r) => r.id === selectedRepoId);
      if (selectedRepo) {
        setCreatedRepoId(selectedRepo.id);
        setCreatedRepoHasSecret(selectedRepo.webhookRegistered);
      } else {
        // New repo was created — re-fetch to get its id.
        const freshRepos = await api.repos();
        const match = freshRepos.find((r) => r.upstream === upstream);
        if (match) {
          setCreatedRepoId(match.id);
          setCreatedRepoHasSecret(match.webhookRegistered);
        }
      }
      setStep(3);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setCreating(false);
    }
  };

  const empty = projects.length === 0;

  // Steps indicator
  const steps = [
    { n: 1, label: "repo" },
    { n: 2, label: "project" },
    { n: 3, label: "webhook" },
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
            repos={repos}
            selectedRepoId={selectedRepoId}
            newUrl={newUrl}
            onSelect={setSelectedRepoId}
            onNewUrl={setNewUrl}
            onNext={() => setStep(2)}
          />
        )}

        {step === 2 && (
          <StepProject
            repoUpstream={upstream}
            name={name}
            onName={setName}
            creating={creating}
            error={error}
            onBack={() => setStep(1)}
            onSubmit={handleCreate}
          />
        )}

        {step === 3 && createdRepoId != null && (
          <StepWebhook
            repoId={createdRepoId}
            repoUpstream={upstream}
            webhookRegistered={createdRepoWebhookRegistered}
            onDone={() => navigate("/")}
          />
        )}

        {/* Cancel link for non-empty state */}
        {!empty && step < 3 && (
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
