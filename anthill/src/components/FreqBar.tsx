import { useTheme } from "../lib/theme";
import { MONO } from "../lib/format";

export function FreqBar({ value, max }: { value: number; max: number }) {
  const { C } = useTheme();
  const pct = max > 0 ? Math.round((value / max) * 100) : 0;
  const col = pct >= 70 ? C.red : pct >= 40 ? C.amber : C.accent;
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
      <div
        style={{
          flex: 1,
          height: 4,
          background: C.surface3,
          borderRadius: 2,
          overflow: "hidden",
        }}
      >
        <div
          style={{
            width: `${pct}%`,
            height: "100%",
            background: col,
            borderRadius: 2,
          }}
        />
      </div>
      <span
        style={{
          fontSize: 10,
          color: col,
          minWidth: 24,
          textAlign: "right",
          fontFamily: MONO,
        }}
      >
        {value}
      </span>
    </div>
  );
}