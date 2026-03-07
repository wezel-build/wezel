import { useTheme } from "../lib/theme";

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
      className="flex items-center gap-[10px] text-[9px] font-mono"
      style={{ color: C.textDim }}
    >
      <span className="font-bold tracking-[0.5px] uppercase">rebuild freq</span>
      {stops.map((s) => {
        const c = heatColor(s.heat);
        return (
          <div key={s.label} className="flex items-center gap-[3px]">
            <div
              className="w-[8px] h-[8px] rounded-sm"
              style={{
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
