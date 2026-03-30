import { useMemo } from "react";
import { useSearchParams, Link } from "react-router-dom";
import { ArrowRight } from "lucide-react";
import { useCompare } from "../lib/hooks";
import { C, alpha } from "../lib/colors";
import { fmtValue, fmtTime } from "../lib/format";
import { DeltaBadge } from "../components/DeltaBadge";
import { useProject } from "../lib/useProject";
import type { ForagerCommit, Measurement } from "../lib/data";

const GRID =
  "grid grid-cols-[1fr_110px_110px_120px] gap-[8px] items-center";

function CommitTag({
  commit,
  label,
  projectId,
}: {
  commit: ForagerCommit;
  label: string;
  projectId: number;
}) {
  return (
    <Link
      to={`/project/${projectId}/commit/${commit.shortSha}`}
      className="no-underline flex items-center gap-[6px]"
    >
      <span
        className="text-[10px] font-bold uppercase tracking-[0.6px] px-[5px] py-[3px] rounded-[3px] border"
        style={{
          color: label === "base" ? C.cyan : C.pink,
          background: alpha(label === "base" ? C.cyan : C.pink, 8),
          borderColor: alpha(label === "base" ? C.cyan : C.pink, 20),
        }}
      >
        {label}
      </span>
      <span className="font-mono text-[11px] font-semibold text-c-pink">
        {commit.shortSha}
      </span>
      <span className="text-xs text-fg overflow-hidden text-ellipsis whitespace-nowrap">
        {commit.message}
      </span>
    </Link>
  );
}

export default function ComparePage() {
  const [params] = useSearchParams();
  const baseSha = params.get("base_sha");
  const headSha = params.get("head_sha");
  const { current } = useProject();
  const { compare, loading, error } = useCompare(baseSha, headSha);

  const pairs = useMemo(() => {
    if (!compare) return [];
    const baseMap = new Map(
      compare.base.measurements.map((m) => [m.name, m]),
    );
    const headMap = new Map(
      compare.head.measurements.map((m) => [m.name, m]),
    );
    const allNames = new Set([...baseMap.keys(), ...headMap.keys()]);
    return Array.from(allNames)
      .sort()
      .map((name) => ({
        name,
        base: baseMap.get(name),
        head: headMap.get(name),
      }));
  }, [compare]);

  if (!baseSha || !headSha) {
    return (
      <div className="flex-1 flex items-center justify-center text-[11px] text-dim font-mono">
        missing base_sha or head_sha query params
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-y-auto bg-bg">
      <div className="max-w-[800px] mx-auto p-[16px] flex flex-col gap-[12px]">
        {/* Header */}
        <div className="flex items-center gap-[10px] border border-[var(--c-border)] rounded-md bg-surface px-[12px] py-[10px]">
          {compare && current ? (
            <>
              <CommitTag
                commit={compare.base}
                label="base"
                projectId={current.id}
              />
              <ArrowRight size={14} color={C.textDim} className="shrink-0" />
              <CommitTag
                commit={compare.head}
                label="head"
                projectId={current.id}
              />
            </>
          ) : (
            <span className="text-xs font-mono text-dim">
              {baseSha} → {headSha}
            </span>
          )}
        </div>

        {loading && (
          <div className="text-[11px] text-dim font-mono px-[12px]">
            loading…
          </div>
        )}
        {error && !loading && (
          <div className="text-[11px] text-c-red font-mono px-[12px]">
            failed: {error}
          </div>
        )}

        {!loading && !error && compare && (
          <div className="border border-[var(--c-border)] rounded-md overflow-hidden bg-surface">
            {/* Column headers */}
            <div
              className={`${GRID} px-[12px] py-[8px] text-[10px] font-bold text-dim uppercase tracking-[0.8px] border-b border-[var(--c-border)] bg-surface2`}
            >
              <span>Measurement</span>
              <span className="text-right">Base</span>
              <span className="text-right">Head</span>
              <span className="text-right">Delta</span>
            </div>

            {pairs.length === 0 && (
              <div className="px-[12px] py-[18px] text-[11px] text-dim font-mono">
                no measurements to compare
              </div>
            )}

            {pairs.map(({ name, base, head }) => (
              <div
                key={name}
                className={`${GRID} px-[12px] py-[8px] border-b border-[var(--c-border)] text-[11px] font-mono`}
              >
                <span className="text-mid overflow-hidden text-ellipsis whitespace-nowrap">
                  {name}
                </span>
                <span className="text-right text-fg">
                  {fmtMeasurement(base)}
                </span>
                <span className="text-right text-fg">
                  {fmtMeasurement(head)}
                </span>
                <span className="flex justify-end">
                  {base?.value != null && head?.value != null ? (
                    <DeltaBadge
                      current={head.value}
                      baseline={base.value}
                      unit={head.unit}
                    />
                  ) : (
                    <span className="text-dim">—</span>
                  )}
                </span>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function fmtMeasurement(m: Measurement | undefined): string {
  if (!m || m.value == null) return "—";
  return fmtValue(m.value, m.unit);
}
