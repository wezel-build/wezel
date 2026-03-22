import { useEffect, useRef } from "react";
import type { VizSpec } from "../lib/data";
import { fmtValue } from "../lib/format";
import { C } from "../lib/colors";

interface Props {
  spec: VizSpec;
  /** Rows of data to inject into the visualization. */
  data: Record<string, unknown>[];
  unit?: string;
}

/**
 * Renders a visualization described by a VizSpec.
 *
 * - "stat"      → a labelled KPI number (no external deps)
 * - "vega-lite" → a Vega-Lite chart via vega-embed (lazy-loaded)
 */
export function VizRenderer({ spec, data, unit }: Props) {
  if (spec.type === "stat") {
    const value = data[0]?.[spec.field] as number | undefined;
    return (
      <div className="flex flex-col gap-[1px]">
        <span
          className="text-[9px] uppercase tracking-[0.8px] font-semibold"
          style={{ color: C.textDim }}
        >
          {spec.label}
        </span>
        <span
          className="text-[15px] font-bold font-mono"
          style={{ color: C.text }}
        >
          {value != null ? fmtValue(value, unit) : "—"}
        </span>
      </div>
    );
  }

  if (spec.type === "vega-lite") {
    return <VegaLiteChart spec={spec.spec} data={data} />;
  }

  return null;
}

function VegaLiteChart({
  spec,
  data,
}: {
  spec: Record<string, unknown>;
  data: Record<string, unknown>[];
}) {
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;

    let cancelled = false;
    let finalize: (() => void) | undefined;

    import("vega-embed").then(({ default: embed }) => {
      if (cancelled || !el) return;
      embed(el, { ...spec, data: { values: data } } as never, {
        actions: false,
      }).then((result) => {
        finalize = () => result.finalize();
      });
    });

    return () => {
      cancelled = true;
      finalize?.();
    };
  }, [spec, data]);

  return <div ref={containerRef} />;
}
