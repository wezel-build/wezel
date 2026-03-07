import { useTheme } from "../lib/theme";

export function FreqBar({ value, max }: { value: number; max: number }) {
  const { C } = useTheme();
  const pct = max > 0 ? Math.round((value / max) * 100) : 0;
  const col = pct >= 70 ? C.red : pct >= 40 ? C.amber : C.accent;
  return (
    <div className="flex items-center gap-[6px]">
      <div
        className="flex-1 h-[4px] rounded-sm overflow-hidden"
        style={{ background: C.surface3 }}
      >
        <div
          className="h-full rounded-sm"
          style={{ width: `${pct}%`, background: col }}
        />
      </div>
      <span
        className="text-[10px] font-mono min-w-[24px] text-right"
        style={{ color: col }}
      >
        {value}
      </span>
    </div>
  );
}
