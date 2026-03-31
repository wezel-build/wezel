import { useState, useEffect, useMemo } from "react";
import { X, ExternalLink } from "lucide-react";
import type {
  Observation,
  RegistryTemplate,
  RegistryAdapter,
} from "../lib/data";
import { detectToolchain } from "../lib/toolchain";
import {
  fetchAdapter,
  generateBenchmarkToml,
  sortedCrateNames,
} from "../lib/registry";
import { benchmarkPrApi } from "../lib/api";
import { computeHeat } from "../lib/data";
import { C } from "../lib/colors";

interface Props {
  observation: Observation;
  projectId: number;
  initialCrate?: string;
  onClose: () => void;
}

export function BenchmarkCreatorModal({
  observation,
  projectId,
  initialCrate,
  onClose,
}: Props) {
  const [adapter, setAdapter] = useState<RegistryAdapter | null>(null);
  const [loadingAdapter, setLoadingAdapter] = useState(true);
  const [selectedTemplate, setSelectedTemplate] =
    useState<RegistryTemplate | null>(null);
  const [fieldValues, setFieldValues] = useState<Record<string, string>>({});
  const [benchmarkName, setBenchmarkName] = useState("");
  const [creating, setCreating] = useState(false);
  const [prUrl, setPrUrl] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const heat = useMemo(
    () =>
      computeHeat(
        observation.runs,
        observation.graph.map((c) => c.name),
      ),
    [observation],
  );
  const crateNames = useMemo(
    () => sortedCrateNames(observation.graph, heat),
    [observation.graph, heat],
  );

  const toolchain = detectToolchain(observation.name);

  useEffect(() => {
    if (!toolchain) {
      setLoadingAdapter(false);
      return;
    }
    fetchAdapter(toolchain).then((a) => {
      setAdapter(a);
      setLoadingAdapter(false);
    });
  }, [toolchain]);

  // Pre-populate patchCrate from initialCrate
  useEffect(() => {
    if (initialCrate) {
      setFieldValues((prev) => ({ ...prev, patchCrate: initialCrate }));
    }
  }, [initialCrate]);

  // Set default benchmark name when template changes
  useEffect(() => {
    if (selectedTemplate) {
      setBenchmarkName(selectedTemplate.id);
      // Set field defaults
      const defaults: Record<string, string> = {};
      for (const field of selectedTemplate.uiSchema?.fields ?? []) {
        if (field.default) defaults[field.id] = field.default;
        if (field.id === "patchCrate" && initialCrate)
          defaults[field.id] = initialCrate;
      }
      setFieldValues(defaults);
    }
  }, [selectedTemplate, initialCrate]);

  const benchmarkToml = useMemo(() => {
    if (!selectedTemplate || !benchmarkName) return "";
    return generateBenchmarkToml(benchmarkName, selectedTemplate, fieldValues);
  }, [selectedTemplate, benchmarkName, fieldValues]);

  const benchmarkPath = `.wezel/benchmarks/${benchmarkName}/benchmark.toml`;

  async function handleCreatePr() {
    if (!benchmarkName || !benchmarkToml) return;
    setCreating(true);
    setError(null);
    try {
      const result = await benchmarkPrApi(projectId).createPr(benchmarkName, {
        [benchmarkPath]: benchmarkToml,
      });
      setPrUrl(result.prUrl);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Failed to create PR");
    } finally {
      setCreating(false);
    }
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center"
      style={{ background: "rgba(0,0,0,0.6)" }}
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        className="flex flex-col rounded-lg shadow-2xl overflow-hidden"
        style={{
          background: "var(--c-surface)",
          border: "1px solid var(--c-border)",
          width: 700,
          maxHeight: "85vh",
        }}
      >
        {/* Header */}
        <div
          className="flex items-center justify-between px-4 py-3 shrink-0"
          style={{ borderBottom: "1px solid var(--c-border)" }}
        >
          <span className="text-[13px] font-semibold font-mono text-fg">
            Create benchmark
          </span>
          <button
            onClick={onClose}
            className="bg-transparent border-none cursor-pointer text-dim p-[2px] flex"
          >
            <X size={14} />
          </button>
        </div>

        {/* Body */}
        <div className="flex flex-1 min-h-0 overflow-hidden">
          {/* Left: template selection + config */}
          <div className="flex flex-col flex-1 min-w-0 overflow-y-auto p-4 gap-4">
            {loadingAdapter ? (
              <div className="text-dim text-[11px] font-mono">
                Loading adapter…
              </div>
            ) : !adapter ? (
              <div className="text-dim text-[11px] font-mono">
                No adapter found for toolchain{" "}
                <span className="font-semibold text-fg">
                  {toolchain ?? "(unknown)"}
                </span>
                .
              </div>
            ) : (
              <>
                {/* Template cards */}
                {!selectedTemplate ? (
                  <div className="flex flex-col gap-2">
                    <div className="text-[10px] font-semibold uppercase tracking-wider text-dim font-mono mb-1">
                      Select template
                    </div>
                    {adapter.templates.map((t) => (
                      <button
                        key={t.id}
                        onClick={() => setSelectedTemplate(t)}
                        className="text-left rounded p-3 cursor-pointer transition-colors"
                        style={{
                          background: "var(--c-surface2)",
                          border: "1px solid var(--c-border)",
                        }}
                        onMouseEnter={(e) =>
                          (e.currentTarget.style.borderColor = C.accent)
                        }
                        onMouseLeave={(e) =>
                          (e.currentTarget.style.borderColor = C.border)
                        }
                      >
                        <div className="text-[12px] font-semibold text-fg font-mono">
                          {t.name}
                        </div>
                        <div className="text-[11px] text-dim mt-[2px]">
                          {t.description}
                        </div>
                      </button>
                    ))}
                  </div>
                ) : (
                  <div className="flex flex-col gap-3">
                    <button
                      onClick={() => setSelectedTemplate(null)}
                      className="text-[10px] text-accent font-mono bg-transparent border-none cursor-pointer p-0 text-left"
                    >
                      ← Back to templates
                    </button>

                    <div className="text-[12px] font-semibold text-fg font-mono">
                      {selectedTemplate.name}
                    </div>

                    {/* Benchmark name */}
                    <label className="flex flex-col gap-[4px]">
                      <span className="text-[10px] font-semibold uppercase tracking-wider text-dim font-mono">
                        Benchmark name
                      </span>
                      <input
                        value={benchmarkName}
                        onChange={(e) =>
                          setBenchmarkName(
                            e.target.value.replace(/[^a-z0-9-_]/gi, "-"),
                          )
                        }
                        className="bg-surface2 border border-[var(--c-border)] rounded px-2 py-1 text-[11px] font-mono text-fg outline-none"
                        style={{ colorScheme: "dark" }}
                      />
                    </label>

                    {/* UI schema fields */}
                    {(selectedTemplate.uiSchema?.fields ?? []).map((field) => (
                      <label key={field.id} className="flex flex-col gap-[4px]">
                        <span className="text-[10px] font-semibold uppercase tracking-wider text-dim font-mono">
                          {field.label}
                          {field.description && (
                            <span className="normal-case font-normal ml-1 text-[9px]">
                              — {field.description}
                            </span>
                          )}
                        </span>
                        {field.type === "crate-picker" ? (
                          <select
                            value={fieldValues[field.id] ?? ""}
                            onChange={(e) =>
                              setFieldValues((prev) => ({
                                ...prev,
                                [field.id]: e.target.value,
                              }))
                            }
                            className="bg-surface2 border border-[var(--c-border)] rounded px-2 py-1 text-[11px] font-mono text-fg outline-none"
                            style={{ colorScheme: "dark" }}
                          >
                            <option value="">— pick a crate —</option>
                            {crateNames.map((name) => (
                              <option key={name} value={name}>
                                {name} ({heat[name] ?? 0}% dirty)
                              </option>
                            ))}
                          </select>
                        ) : field.type === "select" ? (
                          <select
                            value={fieldValues[field.id] ?? field.default ?? ""}
                            onChange={(e) =>
                              setFieldValues((prev) => ({
                                ...prev,
                                [field.id]: e.target.value,
                              }))
                            }
                            className="bg-surface2 border border-[var(--c-border)] rounded px-2 py-1 text-[11px] font-mono text-fg outline-none"
                            style={{ colorScheme: "dark" }}
                          >
                            {(field.options ?? []).map((opt) => (
                              <option key={opt} value={opt}>
                                {opt}
                              </option>
                            ))}
                          </select>
                        ) : (
                          <input
                            value={fieldValues[field.id] ?? field.default ?? ""}
                            onChange={(e) =>
                              setFieldValues((prev) => ({
                                ...prev,
                                [field.id]: e.target.value,
                              }))
                            }
                            className="bg-surface2 border border-[var(--c-border)] rounded px-2 py-1 text-[11px] font-mono text-fg outline-none"
                          />
                        )}
                      </label>
                    ))}
                  </div>
                )}
              </>
            )}
          </div>

          {/* Right: preview pane */}
          {selectedTemplate && (
            <div
              className="w-[280px] shrink-0 flex flex-col overflow-hidden"
              style={{ borderLeft: "1px solid var(--c-border)" }}
            >
              <div
                className="px-3 py-2 text-[9px] font-semibold uppercase tracking-wider text-dim font-mono shrink-0"
                style={{ borderBottom: "1px solid var(--c-border)" }}
              >
                {benchmarkPath || ".wezel/benchmarks/…/benchmark.toml"}
              </div>
              <pre
                className="flex-1 overflow-auto p-3 text-[10px] font-mono text-fg m-0"
                style={{ background: "var(--c-surface2)" }}
              >
                {benchmarkToml}
              </pre>
            </div>
          )}
        </div>

        {/* Footer */}
        {selectedTemplate && (
          <div
            className="flex items-center justify-between px-4 py-3 shrink-0"
            style={{ borderTop: "1px solid var(--c-border)" }}
          >
            {error && (
              <span className="text-[11px] font-mono" style={{ color: C.red }}>
                {error}
              </span>
            )}
            {prUrl ? (
              <a
                href={prUrl}
                target="_blank"
                rel="noopener noreferrer"
                className="flex items-center gap-[5px] text-[11px] font-mono text-accent no-underline ml-auto"
              >
                View PR on GitHub <ExternalLink size={11} />
              </a>
            ) : (
              <button
                onClick={handleCreatePr}
                disabled={creating || !benchmarkName}
                className="ml-auto px-3 py-[5px] rounded text-[11px] font-mono font-semibold cursor-pointer"
                style={{
                  background: C.accent,
                  color: "#fff",
                  border: "none",
                  opacity: creating || !benchmarkName ? 0.5 : 1,
                }}
              >
                {creating ? "Creating PR…" : "Open PR"}
              </button>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
