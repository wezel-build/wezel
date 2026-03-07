import { useState, useCallback } from "react";
import { useTheme } from "../lib/theme";
import { useDrag } from "../lib/useDrag";

export function PanelHandle({ onDrag }: { onDrag: (delta: number) => void }) {
  const { C } = useTheme();
  const [hover, setHover] = useState(false);

  const onMouseDown = useDrag({
    onDrag: useCallback((dx: number) => onDrag(dx), [onDrag]),
    cursor: "col-resize",
  });

  return (
    <div
      onMouseDown={onMouseDown}
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      className="w-[6px] shrink-0 cursor-col-resize flex justify-center transition-colors duration-100"
      style={{ background: hover ? C.accent + "22" : "transparent" }}
    >
      <div
        className="w-[1px] h-full transition-colors duration-100"
        style={{ background: hover ? C.accent : C.border }}
      />
    </div>
  );
}
