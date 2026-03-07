import { useTheme } from "../lib/theme";

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
    <div className="flex flex-col gap-[1px]">
      <span
        className="text-[9px] uppercase tracking-[0.8px] font-semibold"
        style={{ color: C.textDim }}
      >
        {label}
      </span>
      <span className="text-[15px] font-bold font-mono" style={{ color }}>
        {value}
      </span>
    </div>
  );
}
