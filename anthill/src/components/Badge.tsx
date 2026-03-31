import type React from "react";
import { alpha } from "../lib/colors";

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
      className="text-[10px] font-semibold tracking-[0.6px] py-[3px] px-[7px] rounded-[3px] uppercase border"
      style={{
        background: bg,
        color,
        borderColor: alpha(color, 20),
      }}
    >
      {children}
    </span>
  );
}
