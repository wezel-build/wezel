import type React from "react";
import { C, alpha } from "../lib/colors";
import { fmtValue } from "../lib/format";

export function DeltaBadge({
  current,
  baseline,
  unit,
  style,
}: {
  current: number;
  baseline: number;
  unit?: string;
  style?: React.CSSProperties;
}) {
  const diff = current - baseline;
  const pct = baseline !== 0 ? Math.round((diff / baseline) * 100) : 0;
  const isRegression = diff > 0;
  const color = diff === 0 ? C.textDim : isRegression ? C.red : C.green;
  const sign = diff > 0 ? "+" : "";

  if (diff === 0) return null;

  return (
    <span
      className="text-[10px] font-mono font-semibold rounded-[3px] py-[1px] px-[5px] whitespace-nowrap border"
      style={{
        color,
        background: alpha(color, 8),
        borderColor: alpha(color, 20),
        ...style,
      }}
    >
      {sign}
      {fmtValue(diff, unit)} ({sign}
      {pct}%)
    </span>
  );
}
