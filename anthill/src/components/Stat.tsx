export function Stat({
  label,
  value,
  color,
}: {
  label: string;
  value: string;
  color: string;
}) {
  return (
    <div className="flex flex-col gap-[1px]">
      <span
        className="text-[9px] uppercase tracking-[0.8px] font-semibold"
        style={{ color: "var(--c-text-dim)" }}
      >
        {label}
      </span>
      <span className="text-[15px] font-bold font-mono" style={{ color }}>
        {value}
      </span>
    </div>
  );
}
