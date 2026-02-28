import { useTheme } from "../lib/theme";
import { MONO } from "../lib/format";

export function HeatLegend() {
  const { C, heatColor } = useTheme();
  const stops = [
    { label: "cold", heat: 5 },
    { label: "low", heat: 25 },
    { label: "mid", heat: 45 },
    { label: "warm", heat: 65 },
    { label: "hot", heat: 90 },
  ];
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 10,
        fontSize: 9,
        color: C.textDim,
        fontFamily: MONO,
      }}
    >
      <span
        style={{
          fontWeight: 700,
          letterSpacing: 0.5,
          textTransform: "uppercase",
        }}
      >
        rebuild freq
      </span>
      {stops.map((s) => {
        const c = heatColor(s.heat);
        return (
          <div
            key={s.label}
            style={{ display: "flex", alignItems: "center", gap: 3 }}
          >
            <div
              style={{
                width: 8,
                height: 8,
                borderRadius: 2,
                background: c.bg,
                border: `1.5px solid ${c.border}`,
              }}
            />
            <span style={{ color: c.text }}>{s.label}</span>
          </div>
        );
      })}
    </div>
  );
}