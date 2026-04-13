import { useNavigate } from "react-router-dom";
import { useProject } from "../lib/useProject";
import { Workflow } from "lucide-react";
import { C } from "../lib/colors";

export default function NewProjectPage() {
  const navigate = useNavigate();
  const { projects } = useProject();

  return (
    <div className="flex-1 flex items-center justify-center p-[24px]">
      <div className="w-full max-w-[460px]">
        <div className="flex items-center gap-[8px] mb-[24px] justify-center">
          <Workflow size={20} color={C.accent} strokeWidth={2.5} />
          <span className="text-base font-extrabold text-accent tracking-[-0.5px] font-sans">
            {projects.length === 0 ? "Welcome to wezel" : "Add a project"}
          </span>
        </div>

        <p className="text-[12px] font-mono text-mid m-0 mb-[16px] text-center">
          Projects are created from your repository using the wezel CLI.
        </p>

        <div className="bg-surface2 border border-[var(--c-border)] rounded-md px-[12px] py-[10px] mb-[20px]">
          <div className="text-[10px] font-mono font-bold text-dim uppercase tracking-[0.8px] mb-[6px]">
            In your repository, run:
          </div>
          <pre className="bg-bg rounded px-[10px] py-[6px] text-[11px] font-mono text-fg m-0 overflow-x-auto">
            wezel setup
          </pre>
          <p className="text-[10px] font-mono text-dim m-0 mt-[6px]">
            This creates <code className="text-mid">.wezel/config.toml</code>{" "}
            with a stable project ID and registers it with this server.
          </p>
        </div>

        <p className="text-[11px] font-mono text-dim m-0 text-center">
          Once registered, the project will appear here automatically.
        </p>

        {projects.length > 0 && (
          <div className="text-center mt-[16px]">
            <button
              type="button"
              onClick={() => navigate("/")}
              className="text-[10px] font-mono text-dim bg-transparent border-none cursor-pointer underline"
            >
              back to dashboard
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
