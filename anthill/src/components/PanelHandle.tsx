import { useState, useCallback } from "react";
import { C, alpha } from "../lib/colors";
import { useDrag } from "../lib/useDrag";

export function PanelHandle({ onDrag }: { onDrag: (delta: number) => void }) {
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
      style={{ background: hover ? alpha(C.accent, 13) : "transparent" }}
    >
      <div
        className="w-[1px] h-full transition-colors duration-100"
        style={{ background: hover ? C.accent : C.border }}
      />
    </div>
  );
}
