import { useState, useCallback } from "react";
import { useTheme } from "../lib/theme";

export function PanelHandle({ onDrag }: { onDrag: (delta: number) => void }) {
  const { C } = useTheme();
  const [hover, setHover] = useState(false);

  const onMouseDown = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      let lastX = e.clientX;
      const onMouseMove = (ev: MouseEvent) => {
        const dx = ev.clientX - lastX;
        lastX = ev.clientX;
        onDrag(dx);
      };
      const onMouseUp = () => {
        document.removeEventListener("mousemove", onMouseMove);
        document.removeEventListener("mouseup", onMouseUp);
        document.body.style.cursor = "";
        document.body.style.userSelect = "";
      };
      document.addEventListener("mousemove", onMouseMove);
      document.addEventListener("mouseup", onMouseUp);
      document.body.style.cursor = "col-resize";
      document.body.style.userSelect = "none";
    },
    [onDrag],
  );

  return (
    <div
      onMouseDown={onMouseDown}
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      style={{
        width: 6,
        flexShrink: 0,
        cursor: "col-resize",
        display: "flex",
        justifyContent: "center",
        background: hover ? C.accent + "22" : "transparent",
        transition: "background 0.1s",
      }}
    >
      <div
        style={{
          width: 1,
          height: "100%",
          background: hover ? C.accent : C.border,
          transition: "background 0.1s",
        }}
      />
    </div>
  );
}