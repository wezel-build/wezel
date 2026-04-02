import { useMemo, useState, useCallback, useEffect, useRef } from "react";
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

const ROW_H = 24;
const ROW_GAP = 8;
const PILL_H = 3;
const BAR_GAP = 16; // horizontal gap between dependent bars
const LEFT_PAD = 16;
const TOP_PAD = 28;
const BOT_PAD = 16;
const LABEL_PAD = 8;
const RIGHT_PAD = 220; // space for labels after the last column

// ── Row type ──────────────────────────────────────────────────────────────────

interface Row {
  name: string;
  version?: string;
  heat: number;
  external: boolean;
  tier: Tier;
  barX: number;
  y: number;
}

// ── Layout computation ────────────────────────────────────────────────────────

function computeRows(topo: CrateTopo[], heat: Record<string, number>): Row[] {
  if (topo.length === 0) return [];
  const nameSet = new Set(topo.map((c) => c.name));

  // All three edge kinds. depMap[A] = deps of A (things A needs before it can build).
  const depMap = new Map<string, string[]>();
  for (const c of topo) {
    const all = [
      ...c.deps,
      ...(c.buildDeps ?? []),
      ...(c.devDeps ?? []),
    ].filter((d) => nameSet.has(d));
    depMap.set(c.name, [...new Set(all)]);
  }

  const consumersOf = new Map<string, string[]>();
  for (const c of topo) consumersOf.set(c.name, []);
  for (const [name, deps] of depMap) {
    for (const dep of deps) consumersOf.get(dep)!.push(name);
  }

  // Kahn's topo-sort seeded from foundations (nodes with no deps).
  // Produces build order: foundations first, top-level binaries last.
  const inDeg = new Map(
    topo.map((c) => [c.name, (depMap.get(c.name) ?? []).length]),
  );
  const queue: string[] = topo
    .filter((c) => inDeg.get(c.name) === 0)
    .map((c) => c.name);
  const topoOrder: string[] = [];
  let qi = 0;
  while (qi < queue.length) {
    const n = queue[qi++];
    topoOrder.push(n);
    for (const consumer of consumersOf.get(n) ?? []) {
      const nd = inDeg.get(consumer)! - 1;
      inDeg.set(consumer, nd);
      if (nd === 0) queue.push(consumer);
    }
  }
  // Cycle fallback.
  for (const c of topo) if (!topoOrder.includes(c.name)) topoOrder.push(c.name);

  // ALAP depth: how many hops from this crate to any consumer (top-level app).
  // Computed using the reverse topoOrder (consumers first).
  // depth[n] = 0 for top-level consumers, increases toward foundations.
  const depth = new Map<string, number>();
  for (const name of [...topoOrder].reverse()) {
    const consumers = consumersOf.get(name) ?? [];
    depth.set(
      name,
      consumers.length === 0
        ? 0
        : Math.min(...consumers.map((c) => depth.get(c) ?? 0)) + 1,
    );
  }

  const maxDepth = Math.max(0, ...depth.values());

  // Column x positions: col[d] starts after the widest bar in col[d-1] + BAR_GAP.
  // Consumers (depth=0) are leftmost; foundations (depth=maxDepth) are rightmost.
  const colMaxW = new Map<number, number>();
  for (const c of topo) {
    const d = depth.get(c.name) ?? 0;
    const w = getTier(heat[c.name] ?? 0).barW;
    colMaxW.set(d, Math.max(colMaxW.get(d) ?? 0, w));
  }
  const colX = new Map<number, number>();
  let x = LEFT_PAD;
  for (let d = 0; d <= maxDepth; d++) {
    colX.set(d, x);
    x += (colMaxW.get(d) ?? MAX_BAR_W) + BAR_GAP;
  }

  // Sort: consumers first (depth 0), then by heat desc within same depth.
  const sorted = [...topo].sort((a, b) => {
    const da = depth.get(a.name) ?? 0;
    const db = depth.get(b.name) ?? 0;
    if (da !== db) return da - db;
    return (heat[b.name] ?? 0) - (heat[a.name] ?? 0);
  });

  return sorted.map((c, i) => ({
    name: c.name,
    version: c.version,
    heat: heat[c.name] ?? 0,
    external: c.external ?? false,
    tier: getTier(heat[c.name] ?? 0),
    barX: colX.get(depth.get(c.name) ?? 0) ?? LEFT_PAD,
    y: TOP_PAD + i * (ROW_H + ROW_GAP),
  }));
}

// ── Component ─────────────────────────────────────────────────────────────────

interface ContextMenu {
  x: number;
  y: number;
  crate: string;
}

export function BuildTimingsChart({
  topo,
  heat,
  heatColor,
  highlightedCrates,
  focusedCrate,
  onNodeClick,
  onNodeFocus,
  onExperiment,
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
  onExperiment?: (name: string) => void;
  bg: string;
  border: string;
  accentColor?: string;
}) {
  const [hoveredCrate, setHoveredCrate] = useState<string | null>(null);
  const [contextMenu, setContextMenu] = useState<ContextMenu | null>(null);
  const menuRef = useRef<HTMLDivElement>(null);

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
    () =>
      rows.length === 0
        ? 100
        : Math.max(...rows.map((r) => r.y)) + ROW_H + BOT_PAD,
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

  const handleContextMenu = useCallback(
    (e: React.MouseEvent) => {
      if (!onExperiment) return;
      const el = (e.target as HTMLElement).closest(
        "[data-crate]",
      ) as HTMLElement | null;
      if (!el) return;
      e.preventDefault();
      setContextMenu({ x: e.clientX, y: e.clientY, crate: el.dataset.crate! });
    },
    [onExperiment],
  );

  // Dismiss context menu on outside click
  useEffect(() => {
    if (!contextMenu) return;
    const handler = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setContextMenu(null);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [contextMenu]);

  const dimmed = activeSet !== null;

  return (
    <>
      <div
        className="w-full h-full overflow-auto rounded"
        style={{
          background: bg,
          border: `1px solid ${border}`,
        }}
        onMouseLeave={() => setHoveredCrate(null)}
      >
        <svg
          width={svgW}
          height={svgH}
          style={{ display: "block" }}
          onMouseOver={handleMouseOver}
          onClick={handleClick}
          onContextMenu={handleContextMenu}
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
            ← CONSUMERS · · · FOUNDATIONS →
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
                  strokeDasharray={row.external ? "4 3" : undefined}
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
                  {row.version && (
                    <tspan
                      dx={4}
                      fontSize={9}
                      fill={colors.border}
                      opacity={0.7}
                    >
                      v{row.version}
                    </tspan>
                  )}
                  <tspan dx={5} fontSize={9} fill={colors.border}>
                    {row.heat}%
                  </tspan>
                </text>
              </g>
            );
          })}
        </svg>
      </div>

      {contextMenu && (
        <div
          ref={menuRef}
          style={{
            position: "fixed",
            top: contextMenu.y,
            left: contextMenu.x,
            zIndex: 200,
            background: "var(--c-surface)",
            border: "1px solid var(--c-border)",
            borderRadius: 6,
            boxShadow: "0 4px 12px rgba(0,0,0,0.35)",
            minWidth: 200,
          }}
        >
          <button
            onClick={() => {
              onExperiment?.(contextMenu.crate);
              setContextMenu(null);
            }}
            style={{
              display: "flex",
              alignItems: "center",
              gap: 6,
              width: "100%",
              background: "transparent",
              border: "none",
              cursor: "pointer",
              padding: "8px 12px",
              fontFamily: "var(--font-mono, monospace)",
              fontSize: 11,
              color: "var(--c-text)",
              textAlign: "left",
            }}
          >
            🚀 Experiment for <strong>{contextMenu.crate}</strong>
          </button>
        </div>
      )}
    </>
  );
}
