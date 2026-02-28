import type React from "react";

export function Badge({
  children,
  color,
  bg,
}: {
  children: React.ReactNode;
  color: string;
  bg: string;
}) {
  return (
    <span
      style={{
        fontSize: 10,
        fontWeight: 600,
        letterSpacing: 0.6,
        padding: "1px 6px",
        borderRadius: 3,
        background: bg,
        color,
        border: `1px solid ${color}33`,
        textTransform: "uppercase",
      }}
    >
      {children}
    </span>
  );
}