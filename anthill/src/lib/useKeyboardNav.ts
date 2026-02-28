import { useEffect, useCallback } from "react";

type KeyMap = Record<string, (e: KeyboardEvent) => void>;

/**
 * Global keydown listener. Ignores events when typing in inputs
 * (except Escape, which always fires).
 */
export function useKeyboardNav(map: KeyMap) {
  const handler = useCallback(
    (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement)?.tagName;
      const isInput = tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT";
      if (isInput && e.key !== "Escape") return;

      const fn = map[e.key];
      if (fn) {
        e.preventDefault();
        fn(e);
      }
    },
    [map],
  );

  useEffect(() => {
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [handler]);
}