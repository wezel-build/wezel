import { useMemo, useState, useCallback } from "react";
import { MONO } from "../lib/format";
import type { CrateTopo } from "../lib/data";
import type { HeatFn } from "../lib/theme";

// ── Tier definitions ──────────────────────────────────────────────────────────

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

const MAX_BAR_W = TIERS[TIERS.length - 1].barW;

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
const PILL_H = 3;
const COL_GAP = 22;
// horizontal gap between depth columns
const LEFT_PAD = 16;
const TOP_PAD = 28;
const BOT_PAD = 16;
const LABEL_PAD = 8;
const RIGHT_PAD = 220; // space for labels after the last column

// ── Row type ──────────────────────────────────────────────────────────────────

interface Row {
  name: string;
  heat: number;
  depth: number;
  tier: Tier;
  barX: number;
  y: number;
}

// ── Layout computation ────────────────────────────────────────────────────────

function computeRows(topo: CrateTopo[], heat: Record<string, number>): Row[] {
  // Only workspace crates become rows.
  const internal = topo.filter((c) => !c.external);
  const nameSet = new Set(internal.map((c) => c.name));

  // Structural deps: normal + build only (dev deps excluded to avoid cycles).
  // Filtered to workspace-only.
  const depMap = new Map<string, string[]>();
  for (const c of internal) {
    const structural = [...c.deps, ...(c.buildDeps ?? [])].filter((d) =>
      nameSet.has(d),
    );
    depMap.set(c.name, structural);
  }

  // Reverse map: for each crate, which crates consume it (depend on it).
  const consumersOf = new Map<string, string[]>();
  for (const c of internal) {
    for (const dep of depMap.get(c.name) ?? []) {
      if (!consumersOf.has(dep)) consumersOf.set(dep, []);
      consumersOf.get(dep)!.push(c.name);
    }
  }

  // Longest-path depth from roots via Kahn's topo-sort + relaxation.
  //
  // BFS (shortest path) would put B at depth 1 even when A→C→B exists,
  // because A also depends on B directly.  We want depth = longest path
  // from any root, so that B sits one tier below C — reflecting that a
  // change to B cascades through C before reaching A.
  //
  // Algorithm:
  //   1. Kahn's topo-sort starting from roots (no consumers), following
  //      dependency edges (root → dep → sub-dep …).
  //   2. Walk nodes in that order and relax: depth[dep] = max(depth[dep], depth[n]+1).

  // Step 1 – Kahn's topo-sort.
  const inDeg = new Map<string, number>();
  for (const c of internal) {
    inDeg.set(c.name, (consumersOf.get(c.name) ?? []).length);
  }

  const topoOrder: string[] = [];
  const tq: string[] = [];
  for (const c of internal) {
    if (inDeg.get(c.name) === 0) tq.push(c.name);
  }
  let tqi = 0;
  while (tqi < tq.length) {
    const name = tq[tqi++];
    topoOrder.push(name);
    for (const dep of depMap.get(name) ?? []) {
      const nd = (inDeg.get(dep) ?? 1) - 1;
      inDeg.set(dep, nd);
      if (nd === 0) tq.push(dep);
    }
  }

  // Step 2 – longest-path relaxation.
  const depths = new Map<string, number>();
  for (const name of topoOrder) {
    const d = depths.get(name) ?? 0;
    for (const dep of depMap.get(name) ?? []) {
      depths.set(dep, Math.max(depths.get(dep) ?? 0, d + 1));
    }
  }

  // Cycle members (not reached by topo-sort): resolve depth from already-placed
  // neighbours iteratively, then fall back to 0 for pure cycles with no anchor.
  let cycleNodes = internal.filter((c) => !depths.has(c.name));
  let prevSize = -1;
  while (cycleNodes.length > 0 && cycleNodes.length !== prevSize) {
    prevSize = cycleNodes.length;
    const stillUnplaced: typeof cycleNodes = [];
    for (const c of cycleNodes) {
      const placedDeps = (depMap.get(c.name) ?? []).filter((d) =>
        depths.has(d),
      );
      if (placedDeps.length > 0) {
        depths.set(
          c.name,
          Math.max(...placedDeps.map((d) => (depths.get(d) ?? 0) + 1)),
        );
      } else {
        stillUnplaced.push(c);
      }
    }
    cycleNodes = stillUnplaced;
  }
  // Pure cycles with no external anchor fall back to 0.
  for (const c of cycleNodes) {
    depths.set(c.name, 0);
  }

  // Sort: depth asc (roots first), heat desc within same depth.
  const sorted = [...internal].sort((a, b) => {
    const da = depths.get(a.name) ?? 0;
    const db = depths.get(b.name) ?? 0;
    if (da !== db) return da - db;
    return (heat[b.name] ?? 0) - (heat[a.name] ?? 0);
  });

  // Fixed column positions: each depth level gets a column whose width equals
  // the widest bar in that level plus COL_GAP.  This keeps columns compact
  // when most crates are low-frequency, and expands only where needed.
  const depthMaxBarW = new Map<number, number>();
  for (const c of sorted) {
    const d = depths.get(c.name) ?? 0;
    const w = getTier(heat[c.name] ?? 0).barW;
    depthMaxBarW.set(d, Math.max(depthMaxBarW.get(d) ?? 0, w));
  }

  const maxDepth =
    sorted.length > 0
      ? Math.max(...sorted.map((c) => depths.get(c.name) ?? 0))
      : 0;

  const colStart = new Map<number, number>();
  let x = LEFT_PAD;
  for (let d = 0; d <= maxDepth; d++) {
    colStart.set(d, x);
    x += (depthMaxBarW.get(d) ?? MAX_BAR_W) + COL_GAP;
  }

  return sorted.map((c, i) => {
    const depth = depths.get(c.name) ?? 0;
    return {
      name: c.name,
      heat: heat[c.name] ?? 0,
      depth,
      tier: getTier(heat[c.name] ?? 0),
      barX: colStart.get(depth) ?? LEFT_PAD,
      y: TOP_PAD + i * (ROW_H + ROW_GAP),
    };
  });
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

  // Dep and reverse-dep maps for building the transitive active set.
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

  const consumersOf = useMemo(() => {
    const m = new Map<string, string[]>();
    for (const [name, deps] of depMap) {
      for (const dep of deps) {
        if (!m.has(dep)) m.set(dep, []);
        m.get(dep)!.push(name);
      }
    }
    return m;
  }, [depMap]);

  // Hover takes priority over sticky focus.
  const activeSetName = hoveredCrate ?? focusedCrate ?? null;

  // Transitive hull: the active crate + all its deps (right) + all its
  // consumers (left).  Everything outside is dimmed.
  const activeSet = useMemo<Set<string> | null>(() => {
    if (!activeSetName) return null;
    const s = new Set<string>([activeSetName]);
    const qd = [...(depMap.get(activeSetName) ?? [])];
    while (qd.length) {
      const n = qd.shift()!;
      if (!s.has(n)) {
        s.add(n);
        qd.push(...(depMap.get(n) ?? []));
      }
    }
    const qc = [...(consumersOf.get(activeSetName) ?? [])];
    while (qc.length) {
      const n = qc.shift()!;
      if (!s.has(n)) {
        s.add(n);
        qc.push(...(consumersOf.get(n) ?? []));
      }
    }
    return s;
  }, [activeSetName, depMap, consumersOf]);

  const svgW = useMemo(() => {
    if (rows.length === 0) return 400;
    return Math.max(...rows.map((r) => r.barX + r.tier.barW)) + RIGHT_PAD;
  }, [rows]);

  const svgH = useMemo(
    () => TOP_PAD + rows.length * (ROW_H + ROW_GAP) + BOT_PAD,
    [rows],
  );

  const handleMouseOver = useCallback((e: React.MouseEvent) => {
    const el = (e.target as HTMLElement).closest(
      "[data-crate]",
    ) as HTMLElement | null;
    setHoveredCrate(el?.dataset.crate ?? null);
  }, []);

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
        {/* Axis hint */}
        <text
          x={LEFT_PAD}
          y={16}
          fontSize={9}
          fontFamily={MONO}
          fill="#666"
          fontWeight={600}
          style={{ letterSpacing: "0.6px" }}
        >
          ← ROOTS · · · FOUNDATIONS →
        </text>

        {/* Rows */}
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
          const labelX = row.barX + row.tier.barW + LABEL_PAD;
          const midY = row.y + ROW_H / 2 + 4;
          const pAlpha = pillOpacity(row.heat, row.tier);

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

              {/* Pill — thin strip at bottom, opacity = position within tier */}
              {row.tier.hasPill && (
                <rect
                  x={pillX}
                  y={pillY}
                  width={pillW}
                  height={PILL_H}
                  rx={1}
                  fill={colors.border}
                  stroke="none"
                  opacity={pAlpha}
                  style={{ pointerEvents: "none" }}
                />
              )}

              {/* Label: name + exact heat% */}
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
