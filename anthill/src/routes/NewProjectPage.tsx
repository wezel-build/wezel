import { useState, type FormEvent } from "react";
import { useNavigate } from "react-router-dom";
import { C } from "../lib/colors";
import { useProject } from "../lib/useProject";
import { Workflow } from "lucide-react";

export default function NewProjectPage() {
  const navigate = useNavigate();
  const { projects, addProject } = useProject();

  const [name, setName] = useState("");
  const [upstream, setUpstream] = useState("");
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const canSubmit = name.trim() && upstream.trim() && !creating;

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault();
    if (!canSubmit) return;
    setCreating(true);
    setError(null);
    try {
      await addProject(name.trim(), upstream.trim());
      navigate("/");
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setCreating(false);
    }
  };

  const empty = projects.length === 0;

  return (
    <div className="flex-1 flex items-center justify-center p-[24px]">
      <div className="w-full max-w-[400px]">
        {empty && (
          <div className="flex items-center gap-[8px] mb-[24px] justify-center">
            <Workflow size={20} color={C.accent} strokeWidth={2.5} />
            <span className="text-base font-extrabold text-accent tracking-[-0.5px] font-sans">
              Welcome to wezel
            </span>
          </div>
        )}

        <h2 className="text-sm font-mono font-bold text-fg m-0 mb-[4px]">
          {empty ? "Create your first project" : "New project"}
        </h2>
        <p className="text-[11px] font-mono text-dim m-0 mb-[20px]">
          Link a GitHub repository to start tracking builds.
        </p>

        <form onSubmit={handleSubmit}>
          <label className="block text-[10px] font-mono font-bold text-dim uppercase tracking-[0.8px] mb-[4px]">
            Name
          </label>
          <input
            autoFocus
            placeholder="my-project"
            value={name}
            onChange={(e) => setName(e.target.value)}
            className="w-full px-[10px] py-[8px] text-[13px] font-mono bg-surface text-fg border border-[var(--c-border)] rounded-md outline-none mb-[14px]"
          />

          <label className="block text-[10px] font-mono font-bold text-dim uppercase tracking-[0.8px] mb-[4px]">
            GitHub repo URL
          </label>
          <input
            placeholder="https://github.com/org/repo"
            value={upstream}
            onChange={(e) => setUpstream(e.target.value)}
            className="w-full px-[10px] py-[8px] text-[13px] font-mono bg-surface text-fg border border-[var(--c-border)] rounded-md outline-none mb-[20px]"
          />

          {error && (
            <div className="text-[11px] font-mono text-c-red mb-[12px]">
              {error}
            </div>
          )}

          <div className="flex gap-[8px] justify-end">
            {!empty && (
              <button
                type="button"
                onClick={() => navigate("/")}
                className="text-[11px] font-mono px-[16px] py-[7px] bg-transparent text-dim border border-[var(--c-border)] rounded-md cursor-pointer"
              >
                cancel
              </button>
            )}
            <button
              type="submit"
              disabled={!canSubmit}
              className={`text-[11px] font-mono font-bold px-[20px] py-[7px] border-0 rounded-md ${canSubmit ? "cursor-pointer" : "cursor-not-allowed"}`}
              style={{
                background: canSubmit ? C.accent : C.surface2,
                color: canSubmit ? C.bg : C.textDim,
              }}
            >
              {creating ? "creating…" : "create project"}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
