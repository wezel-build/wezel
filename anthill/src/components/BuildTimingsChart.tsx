import { useMemo, useState, useCallback } from "react";
import { MONO } from "../lib/format";
import type { CrateTopo } from "../lib/data";
import type { HeatFn } from "../lib/theme";

// ── Tier definitions ──────────────────────────────────────────────────────────
// Five fixed-width tiers. Tiers 2-5 carry a pill that lights up when the
// crate's frequency is in the upper half of that tier's range.
// e.g. tier 3 covers 21-40%; pill is lit for 31-40%.

interface Tier {
  lo: number;
  hi: number;
  barW: number;
  hasPill: boolean;
}

const TIERS: Tier[] = [
  { lo: 0, hi: 10, barW: 16, hasPill: false },
  { lo: 11, hi: 20, barW: 36, hasPill: true },
  { lo: 21, hi: 40, barW: 72, hasPill: true },
  { lo: 41, hi: 70, barW: 116, hasPill: true },
  { lo: 71, hi: 100, barW: 160, hasPill: true },
];

function getTier(heat: number): Tier {
  return TIERS.find((t) => heat <= t.hi) ?? TIERS[TIERS.length - 1];
}

function pillOpacity(heat: number, tier: Tier): number {
  if (!tier.hasPill) return 0;
  return (heat - tier.lo) / (tier.hi - tier.lo);
}

// ── Layout constants ──────────────────────────────────────────────────────────

const ROW_H = 20;
const ROW_GAP = 4;
const PILL_H = 3; // thin strip at the bottom of the bar
const DEP_GAP = 22; // space between dep effective-end and dependent bar-start
const LEFT_PAD = 16;
const RIGHT_PAD = 220; // room for name labels
const TOP_PAD = 28; // room for axis hint
const BOT_PAD = 16;
const LABEL_PAD = 8;

// The effective width of a crate's indicator.
// Used for critical-path X positioning and connection line endpoints.
function effectiveW(tier: Tier): number {
  return tier.barW;
}

// ── Row layout ────────────────────────────────────────────────────────────────

interface Row {
  name: string;
  heat: number; // 0-100
  depth: number;
  tier: Tier;
  barX: number;
  y: number;
}

function computeRows(topo: CrateTopo[], heat: Record<string, number>): Row[] {
  // Only workspace (non-external) crates appear as rows.
  const internal = topo.filter((c) => !c.external);
  const nameSet = new Set(internal.map((c) => c.name));

  // Filter each crate's deps to internal-only.
  const depMap = new Map<string, string[]>();
  for (const c of internal) {
    depMap.set(
      c.name,
      c.deps.filter((d) => nameSet.has(d)),
    );
  }

  // Iterative topo-depth: depth 0 = crate with no internal deps (foundational).
  // We run repeated passes until nothing changes; this handles arbitrary graphs
  // without recursion and degrades gracefully on cycles (cycle members get 0).
  const depths = new Map<string, number>();
  for (const c of internal) {
    if ((depMap.get(c.name) ?? []).length === 0) depths.set(c.name, 0);
  }
  let changed = true;
  while (changed) {
    changed = false;
    for (const c of internal) {
      const deps = depMap.get(c.name) ?? [];
      if (!deps.every((d) => depths.has(d))) continue;
      const d =
        deps.length === 0
          ? 0
          : Math.max(...deps.map((d) => depths.get(d)!)) + 1;
      if (d !== depths.get(c.name)) {
        depths.set(c.name, d);
        changed = true;
      }
    }
  }
  // Any node not reached (cycle) gets depth 0.
  for (const c of internal) {
    if (!depths.has(c.name)) depths.set(c.name, 0);
  }

  // Primary sort: depth asc (foundational first).
  // Secondary: heat desc (hottest crates rise to the top of their tier group).
  const sorted = [...internal].sort((a, b) => {
    const da = depths.get(a.name) ?? 0;
    const db = depths.get(b.name) ?? 0;
    if (da !== db) return da - db;
    return (heat[b.name] ?? 0) - (heat[a.name] ?? 0);
  });

  // Critical-path X: each bar starts after the rightmost effective-end of its
  // direct deps, mirroring how cargo --timings positions units in time.
  // High-frequency foundation crates (wider indicators) push dependants right.
  const barX = new Map<string, number>();
  for (const c of sorted) {
    const deps = depMap.get(c.name) ?? [];
    if (deps.length === 0) {
      barX.set(c.name, LEFT_PAD);
    } else {
      let maxEnd = LEFT_PAD;
      for (const dep of deps) {
        const dx = barX.get(dep) ?? LEFT_PAD;
        const dTier = getTier(heat[dep] ?? 0);
        maxEnd = Math.max(maxEnd, dx + effectiveW(dTier) + DEP_GAP);
      }
      barX.set(c.name, maxEnd);
    }
  }

  return sorted.map((c, i) => ({
    name: c.name,
    heat: heat[c.name] ?? 0,
    depth: depths.get(c.name) ?? 0,
    tier: getTier(heat[c.name] ?? 0),
    barX: barX.get(c.name) ?? LEFT_PAD,
    y: TOP_PAD + i * (ROW_H + ROW_GAP),
  }));
}

// ── Component ─────────────────────────────────────────────────────────────────

export function BuildTimingsChart({
  topo,
  heat,
  heatColor,
  highlightedCrates,
  focusedCrate,
  onNodeClick,
  onNodeFocus,
  bg,
  border,
  accentColor,
}: {
  topo: CrateTopo[];
  heat: Record<string, number>;
  heatColor: HeatFn;
  highlightedCrates?: Set<string>;
  focusedCrate?: string | null;
  onNodeClick?: (name: string) => void;
  onNodeFocus?: (name: string | null) => void;
  bg: string;
  border: string;
  accentColor?: string;
}) {
  const [hoveredCrate, setHoveredCrate] = useState<string | null>(null);

  const rows = useMemo(() => computeRows(topo, heat), [topo, heat]);

  const rowByName = useMemo(() => {
    const m = new Map<string, Row>();
    for (const r of rows) m.set(r.name, r);
    return m;
  }, [rows]);

  // Internal dep map, rebuilt from topo against the current visible row set.
  const depMap = useMemo(() => {
    const nameSet = new Set(rows.map((r) => r.name));
    const m = new Map<string, string[]>();
    for (const c of topo) {
      if (c.external) continue;
      m.set(
        c.name,
        c.deps.filter((d) => nameSet.has(d)),
      );
    }
    return m;
  }, [topo, rows]);

  // Reverse edges: for each crate, which crates directly depend on it.
  const dependantsMap = useMemo(() => {
    const m = new Map<string, string[]>();
    for (const [name, deps] of depMap) {
      for (const dep of deps) {
        if (!m.has(dep)) m.set(dep, []);
        m.get(dep)!.push(name);
      }
    }
    return m;
  }, [depMap]);

  // Hover takes priority over keyboard/click focus for the active highlight.
  const activeSetName = hoveredCrate ?? focusedCrate ?? null;

  // The "active set" = the hovered/focused crate plus every crate reachable
  // from it transitively in either direction (deps and dependants).
  // Everything outside this set is dimmed.
  const activeSet = useMemo<Set<string> | null>(() => {
    if (!activeSetName) return null;
    const s = new Set<string>([activeSetName]);
    // Upstream (deps)
    const q = [...(depMap.get(activeSetName) ?? [])];
    while (q.length) {
      const n = q.shift()!;
      if (!s.has(n)) {
        s.add(n);
        q.push(...(depMap.get(n) ?? []));
      }
    }
    // Downstream (dependants)
    const q2 = [...(dependantsMap.get(activeSetName) ?? [])];
    while (q2.length) {
      const n = q2.shift()!;
      if (!s.has(n)) {
        s.add(n);
        q2.push(...(dependantsMap.get(n) ?? []));
      }
    }
    return s;
  }, [activeSetName, depMap, dependantsMap]);

  // Bezier connection lines shown on hover/focus: one curve per direct dep,
  // drawn from the dep's effective-end to the active crate's bar-start.
  const connLines = useMemo(() => {
    if (!activeSetName) return [];
    const hovRow = rowByName.get(activeSetName);
    if (!hovRow) return [];
    return (depMap.get(activeSetName) ?? []).flatMap((dep) => {
      const dr = rowByName.get(dep);
      if (!dr) return [];
      return [
        {
          x1: dr.barX + effectiveW(dr.tier),
          y1: dr.y + ROW_H / 2,
          x2: hovRow.barX,
          y2: hovRow.y + ROW_H / 2,
        },
      ];
    });
  }, [activeSetName, rowByName, depMap]);

  const svgW = useMemo(
    () =>
      rows.length === 0
        ? 400
        : Math.max(...rows.map((r) => r.barX + effectiveW(r.tier))) + RIGHT_PAD,
    [rows],
  );
  const svgH = useMemo(
    () => TOP_PAD + rows.length * (ROW_H + ROW_GAP) + BOT_PAD,
    [rows],
  );

  // Hover: walk up from the event target to find the nearest [data-crate] group.
  const handleMouseOver = useCallback((e: React.MouseEvent) => {
    const el = (e.target as HTMLElement).closest(
      "[data-crate]",
    ) as HTMLElement | null;
    setHoveredCrate(el?.dataset.crate ?? null);
  }, []);

  // Click: plain click = sticky focus; Ctrl/Meta = run filter.
  const handleClick = useCallback(
    (e: React.MouseEvent) => {
      const el = (e.target as HTMLElement).closest(
        "[data-crate]",
      ) as HTMLElement | null;
      if (!el) {
        onNodeFocus?.(null);
        return;
      }
      const name = el.dataset.crate!;
      if (e.ctrlKey || e.metaKey) onNodeClick?.(name);
      else onNodeFocus?.(name);
    },
    [onNodeClick, onNodeFocus],
  );

  const dimmed = activeSet !== null;

  return (
    <div
      style={{
        width: "100%",
        height: "100%",
        overflow: "auto",
        background: bg,
        border: `1px solid ${border}`,
        borderRadius: 4,
      }}
      onMouseLeave={() => setHoveredCrate(null)}
    >
      <svg
        width={svgW}
        height={svgH}
        style={{ display: "block" }}
        onMouseOver={handleMouseOver}
        onClick={handleClick}
      >
        {/* Axis hint -------------------------------------------------------- */}
        <text
          x={LEFT_PAD}
          y={16}
          fontSize={9}
          fontFamily={MONO}
          fill="#666"
          fontWeight={600}
          style={{ letterSpacing: "0.6px" }}
        >
          ← FOUNDATIONAL · · · CONSUMERS →
        </text>

        {/* Bezier lines: direct deps → active crate ------------------------- */}
        {connLines.map((ln, i) => {
          const mx = (ln.x1 + ln.x2) / 2;
          return (
            <path
              key={i}
              d={`M ${ln.x1} ${ln.y1} C ${mx} ${ln.y1} ${mx} ${ln.y2} ${ln.x2} ${ln.y2}`}
              fill="none"
              stroke={accentColor ?? "#888"}
              strokeWidth={1.5}
              opacity={0.35}
              style={{ pointerEvents: "none" }}
            />
          );
        })}

        {/* Rows ------------------------------------------------------------- */}
        {rows.map((row) => {
          const colors = heatColor(row.heat);
          const isHl = highlightedCrates?.has(row.name) ?? false;
          const isActive = !dimmed || activeSet!.has(row.name);
          const isFocused = row.name === focusedCrate;
          const isHovered = row.name === hoveredCrate;
          const accent = accentColor ?? colors.border;
          const emphBorder = isHl || isFocused || isHovered;

          const pillX = row.barX + 1;
          const pillY = row.y + ROW_H - PILL_H - 1;
          const pillW = row.tier.barW - 2;
          const labelX = row.barX + effectiveW(row.tier) + LABEL_PAD;
          const midY = row.y + ROW_H / 2 + 4;

          const pillAlpha = pillOpacity(row.heat, row.tier);

          return (
            <g
              key={row.name}
              data-crate={row.name}
              style={{ cursor: "pointer", opacity: isActive ? 1 : 0.12 }}
            >
              {/* Bar */}
              <rect
                x={row.barX}
                y={row.y}
                width={row.tier.barW}
                height={ROW_H}
                rx={3}
                fill={colors.bg}
                stroke={emphBorder ? accent : colors.border}
                strokeWidth={emphBorder ? 2 : 1}
              />

              {/* Pill (tiers 2-5 only) — thin strip inside bar at the bottom;
                  opacity scales linearly with position within the tier range */}
              {row.tier.hasPill && (
                <rect
                  x={pillX}
                  y={pillY}
                  width={pillW}
                  height={PILL_H}
                  rx={1}
                  fill={colors.border}
                  stroke="none"
                  opacity={pillAlpha}
                  style={{ pointerEvents: "none" }}
                />
              )}

              {/* Label: name + exact heat% as a dim tspan */}
              <text
                x={labelX}
                y={midY}
                fontSize={11}
                fontFamily={MONO}
                fill={isActive ? colors.text : "#444"}
                style={{ pointerEvents: "none" }}
              >
                {row.name}
                <tspan dx={5} fontSize={9} fill={colors.border}>
                  {row.heat}%
                </tspan>
              </text>
            </g>
          );
        })}
      </svg>
    </div>
  );
}
