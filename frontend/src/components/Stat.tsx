import { useTheme } from "../lib/theme";
import { MONO } from "../lib/format";

export function Stat({
  label,
  value,
  color,
}: {
  label: string;
  value: string;
  color: string;
}) {
  const { C } = useTheme();
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 1 }}>
      <span
        style={{
          fontSize: 9,
          color: C.textDim,
          textTransform: "uppercase",
          letterSpacing: 0.8,
          fontWeight: 600,
        }}
      >
        {label}
      </span>
      <span style={{ fontSize: 15, fontWeight: 700, color, fontFamily: MONO }}>
        {value}
      </span>
    </div>
  );
}