import type React from "react";
import { useTheme } from "../lib/theme";
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
  const { C } = useTheme();
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
        background: color + "15",
        borderColor: color + "33",
        ...style,
      }}
    >
      {sign}
      {fmtValue(diff, unit)} ({sign}
      {pct}%)
    </span>
  );
}
