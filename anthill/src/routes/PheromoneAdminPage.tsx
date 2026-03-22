import { useState, useEffect } from "react";
import { RefreshCw, Plus } from "lucide-react";
import { api } from "../lib/api";
import type { Pheromone } from "../lib/data";
import { C } from "../lib/colors";

export default function PheromoneAdminPage() {
  const [pheromones, setPheromones] = useState<Pheromone[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [repoInput, setRepoInput] = useState("");
  const [registering, setRegistering] = useState(false);
  const [registerError, setRegisterError] = useState<string | null>(null);
  const [expandedId, setExpandedId] = useState<number | null>(null);
  const [refreshing, setRefreshing] = useState<number | null>(null);

  async function load() {
    try {
      const data = await api.admin.pheromones();
      setPheromones(data);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Failed to load pheromones");
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    load();
  }, []);

  async function handleRegister(e: React.FormEvent) {
    e.preventDefault();
    const repo = repoInput.trim();
    if (!repo) return;
    setRegistering(true);
    setRegisterError(null);
    try {
      const p = await api.admin.registerPheromone(repo);
      setPheromones((prev) => {
        const idx = prev.findIndex((x) => x.id === p.id);
        if (idx >= 0) {
          const next = [...prev];
          next[idx] = p;
          return next;
        }
        return [...prev, p];
      });
      setRepoInput("");
    } catch (e: unknown) {
      setRegisterError(e instanceof Error ? e.message : "Registration failed");
    } finally {
      setRegistering(false);
    }
  }

  async function handleRefresh(name: string, id: number) {
    setRefreshing(id);
    try {
      const p = await api.admin.fetchPheromone(name);
      setPheromones((prev) => prev.map((x) => (x.id === p.id ? p : x)));
    } catch {
      // ignore
    } finally {
      setRefreshing(null);
    }
  }

  return (
    <div className="flex flex-col flex-1 overflow-y-auto p-6 gap-6 max-w-3xl">
      <div>
        <h1 className="text-[16px] font-semibold font-mono text-fg m-0">
          Pheromone Registry
        </h1>
        <p className="text-[12px] text-dim font-mono mt-1 m-0">
          Manage pheromone tools and their field schemas.
        </p>
      </div>

      {/* Register form */}
      <form
        onSubmit={handleRegister}
        className="flex flex-col gap-2 p-4 rounded"
        style={{
          background: "var(--c-surface2)",
          border: "1px solid var(--c-border)",
        }}
      >
        <div className="text-[10px] font-semibold uppercase tracking-wider text-dim font-mono">
          Register pheromone
        </div>
        <div className="flex items-center gap-2">
          <input
            value={repoInput}
            onChange={(e) => setRepoInput(e.target.value)}
            placeholder="e.g. wezelhq/pheromone-cargo"
            className="flex-1 bg-bg border border-[var(--c-border)] rounded px-2 py-1 text-[11px] font-mono text-fg outline-none"
            style={{ colorScheme: "dark" }}
          />
          <button
            type="submit"
            disabled={registering || !repoInput.trim()}
            className="flex items-center gap-[5px] px-3 py-1 rounded text-[11px] font-mono font-semibold cursor-pointer"
            style={{
              background: C.accent,
              color: "#fff",
              border: "none",
              opacity: registering || !repoInput.trim() ? 0.5 : 1,
            }}
          >
            <Plus size={12} />
            {registering ? "Registering…" : "Register"}
          </button>
        </div>
        {registerError && (
          <span className="text-[11px] font-mono" style={{ color: C.red }}>
            {registerError}
          </span>
        )}
      </form>

      {/* Pheromone list */}
      {loading ? (
        <div className="text-dim text-[11px] font-mono">Loading…</div>
      ) : error ? (
        <div className="text-[11px] font-mono" style={{ color: C.red }}>
          {error}
        </div>
      ) : pheromones.length === 0 ? (
        <div className="text-dim text-[11px] font-mono">
          No pheromones registered yet.
        </div>
      ) : (
        <div className="flex flex-col gap-3">
          {pheromones.map((p) => (
            <div
              key={p.id}
              className="rounded overflow-hidden"
              style={{ border: "1px solid var(--c-border)" }}
            >
              {/* Header */}
              <div
                className="flex items-center justify-between px-4 py-2"
                style={{ background: "var(--c-surface2)" }}
              >
                <div className="flex items-center gap-3">
                  <span className="text-[12px] font-semibold font-mono text-fg">
                    {p.name}
                  </span>
                  <span
                    className="text-[10px] font-mono px-[5px] py-[1px] rounded"
                    style={{
                      background: "var(--c-surface3)",
                      color: "var(--c-text-mid)",
                      border: "1px solid var(--c-border)",
                    }}
                  >
                    v{p.version}
                  </span>
                  <a
                    href={`https://github.com/${p.githubRepo}`}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="text-[10px] font-mono text-dim no-underline hover:text-accent"
                  >
                    {p.githubRepo}
                  </a>
                </div>
                <div className="flex items-center gap-2">
                  <button
                    onClick={() => handleRefresh(p.name, p.id)}
                    disabled={refreshing === p.id}
                    title="Refresh schema"
                    className="flex items-center gap-[4px] bg-transparent border-none cursor-pointer text-dim text-[10px] font-mono"
                    style={{ opacity: refreshing === p.id ? 0.5 : 1 }}
                  >
                    <RefreshCw
                      size={11}
                      className={refreshing === p.id ? "animate-spin" : ""}
                    />
                    {refreshing === p.id ? "Fetching…" : "Refresh"}
                  </button>
                  <button
                    onClick={() =>
                      setExpandedId(expandedId === p.id ? null : p.id)
                    }
                    className="bg-transparent border-none cursor-pointer text-dim text-[10px] font-mono"
                  >
                    {expandedId === p.id ? "▲ hide fields" : "▼ fields"}
                  </button>
                </div>
              </div>

              {/* Platforms */}
              <div
                className="flex items-center gap-2 px-4 py-2 flex-wrap"
                style={{ borderTop: "1px solid var(--c-border)" }}
              >
                <span className="text-[9px] font-semibold uppercase tracking-wider text-dim font-mono">
                  platforms
                </span>
                {p.platforms.length === 0 ? (
                  <span className="text-dim text-[10px] font-mono">none</span>
                ) : (
                  p.platforms.map((pl) => (
                    <span
                      key={pl}
                      className="text-[10px] font-mono px-[5px] py-[1px] rounded"
                      style={{
                        background: "var(--c-surface3)",
                        color: "var(--c-text-mid)",
                        border: "1px solid var(--c-border)",
                      }}
                    >
                      {pl}
                    </span>
                  ))
                )}
              </div>

              {/* Fields table */}
              {expandedId === p.id && p.fields.length > 0 && (
                <div style={{ borderTop: "1px solid var(--c-border)" }}>
                  <table className="w-full text-[10px] font-mono border-collapse">
                    <thead>
                      <tr style={{ borderBottom: "1px solid var(--c-border)" }}>
                        {["field", "type", "description", "deprecated"].map(
                          (h) => (
                            <th
                              key={h}
                              className="text-left px-4 py-2 text-[9px] uppercase tracking-wider text-dim font-semibold"
                            >
                              {h}
                            </th>
                          ),
                        )}
                      </tr>
                    </thead>
                    <tbody>
                      {p.fields.map((f) => (
                        <tr
                          key={f.name}
                          style={{
                            borderBottom: "1px solid var(--c-border)",
                            opacity: f.deprecated ? 0.55 : 1,
                          }}
                        >
                          <td className="px-4 py-2 text-fg font-semibold">
                            {f.name}
                          </td>
                          <td className="px-4 py-2 text-dim">{f.type}</td>
                          <td className="px-4 py-2 text-dim">
                            {f.description ?? "—"}
                          </td>
                          <td className="px-4 py-2">
                            {f.deprecated ? (
                              <span style={{ color: C.amber }}>
                                since {f.deprecatedIn ?? "?"}
                                {f.replacedBy && ` → ${f.replacedBy}`}
                              </span>
                            ) : (
                              <span className="text-dim">—</span>
                            )}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
