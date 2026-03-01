import { useRef, useCallback, useMemo, useEffect, useState } from "react";
import { MONO } from "../lib/format";
import type { CrateTopo } from "../lib/data";
import type { HeatFn } from "../lib/theme";

// ── Layout types ─────────────────────────────────────────────────────────────

export interface GraphNode {
  name: string;
  x: number;
  y: number;
  heat: number;
  external: boolean;
  colors: { border: string; bg: string; text: string };
  highlighted: boolean;
}

export interface GraphEdge {
  source: string;
  target: string;
  color: string;
}

// ── Layout computation ───────────────────────────────────────────────────────

const NW = 150;
const NH = 44;
const GX = 32;
const GY = 72;

export function layoutGraph(
  topo: CrateTopo[],
  heat: Record<string, number>,
  heatColor: HeatFn,
  highlightedCrates?: Set<string>,
): { nodes: GraphNode[]; edges: GraphEdge[] } {
  interface Item {
    name: string;
    deps: string[];
    heat: number;
    external?: boolean;
  }

  const items: Item[] = topo.map((c) => ({
    ...c,
    heat: heat[c.name] ?? 0,
  }));

  const nameToItem = new Map<string, Item>();
  const nameSet = new Set<string>();
  for (const c of items) {
    nameToItem.set(c.name, c);
    nameSet.add(c.name);
  }

  // Depths (cycle-safe)
  const depths = new Map<string, number>();
  const visiting = new Set<string>();

  function getDepth(name: string): number {
    if (depths.has(name)) return depths.get(name)!;
    if (visiting.has(name)) return 0;
    visiting.add(name);
    const node = nameToItem.get(name);
    if (!node || node.deps.length === 0) {
      depths.set(name, 0);
      visiting.delete(name);
      return 0;
    }
    let maxChild = -1;
    for (const d of node.deps) {
      if (nameSet.has(d)) {
        const cd = getDepth(d);
        if (cd > maxChild) maxChild = cd;
      }
    }
    const depth = maxChild >= 0 ? 1 + maxChild : 0;
    depths.set(name, depth);
    visiting.delete(name);
    return depth;
  }

  for (const c of items) getDepth(c.name);

  const maxDepth = items.length > 0 ? Math.max(...depths.values()) : 0;
  const layers: string[][] = Array.from({ length: maxDepth + 1 }, () => []);
  for (const c of items) {
    layers[maxDepth - (depths.get(c.name) ?? 0)].push(c.name);
  }

  // Color cache
  const colorCache = new Map<number, ReturnType<HeatFn>>();
  function getCachedColor(h: number) {
    let c = colorCache.get(h);
    if (!c) {
      c = heatColor(h);
      colorCache.set(h, c);
    }
    return c;
  }

  // Position nodes
  const nodePositions = new Map<string, { x: number; y: number }>();
  const nodes: GraphNode[] = [];

  for (let ly = 0; ly < layers.length; ly++) {
    const layer = layers[ly];
    const w = layer.length * NW + (layer.length - 1) * GX;
    for (let ci = 0; ci < layer.length; ci++) {
      const name = layer[ci];
      const item = nameToItem.get(name)!;
      const colors = getCachedColor(item.heat);
      const x = -w / 2 + ci * (NW + GX);
      const y = ly * (NH + GY);
      nodePositions.set(name, { x, y });
      nodes.push({
        name,
        x,
        y,
        heat: item.heat,
        external: item.external ?? false,
        colors,
        highlighted: highlightedCrates?.has(name) ?? false,
      });
    }
  }

  // Transitive reduction
  const reachableCache = new Map<string, Set<string>>();
  function getReachable(name: string): Set<string> {
    let r = reachableCache.get(name);
    if (r) return r;
    r = new Set<string>();
    reachableCache.set(name, r);
    const node = nameToItem.get(name);
    if (!node) return r;
    for (const dep of node.deps) {
      if (!nameSet.has(dep)) continue;
      r.add(dep);
      for (const transitive of getReachable(dep)) {
        r.add(transitive);
      }
    }
    return r;
  }
  for (const c of items) getReachable(c.name);

  const edges: GraphEdge[] = [];
  for (const crate of items) {
    const dominated = new Set<string>();
    for (const dep of crate.deps) {
      if (!nameSet.has(dep)) continue;
      for (const tr of getReachable(dep)) {
        dominated.add(tr);
      }
    }
    const col = getCachedColor(crate.heat);
    for (const dep of crate.deps) {
      if (nameSet.has(dep) && !dominated.has(dep)) {
        edges.push({
          source: crate.name,
          target: dep,
          color: col.border,
        });
      }
    }
  }

  return { nodes, edges };
}

// ── SVG Graph component ──────────────────────────────────────────────────────

interface Point {
  x: number;
  y: number;
}

function usePanZoom(containerRef: React.RefObject<HTMLDivElement | null>) {
  const [transform, setTransform] = useState({ x: 0, y: 0, k: 1 });
  const transformRef = useRef(transform);
  useEffect(() => {
    transformRef.current = transform;
  }, [transform]);

  const dragging = useRef<{
    startX: number;
    startY: number;
    ox: number;
    oy: number;
  } | null>(null);

  const onWheel = useCallback((e: React.WheelEvent) => {
    e.preventDefault();
    const t = transformRef.current;
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const mx = e.clientX - rect.left;
    const my = e.clientY - rect.top;

    const factor = e.deltaY < 0 ? 1.1 : 1 / 1.1;
    const newK = Math.min(4, Math.max(0.05, t.k * factor));
    const ratio = newK / t.k;

    setTransform({
      k: newK,
      x: mx - (mx - t.x) * ratio,
      y: my - (my - t.y) * ratio,
    });
  }, []);

  const onMouseDown = useCallback((e: React.MouseEvent) => {
    if (e.button !== 0) return;
    // Don't start pan if clicking on a node
    const target = e.target as HTMLElement;
    if (target.closest("[data-crate]")) return;
    e.preventDefault();
    const t = transformRef.current;
    dragging.current = {
      startX: e.clientX,
      startY: e.clientY,
      ox: t.x,
      oy: t.y,
    };
  }, []);

  const onMouseMove = useCallback((e: React.MouseEvent) => {
    if (!dragging.current) return;
    const d = dragging.current;
    setTransform((t) => ({
      ...t,
      x: d.ox + (e.clientX - d.startX),
      y: d.oy + (e.clientY - d.startY),
    }));
  }, []);

  const onMouseUp = useCallback(() => {
    dragging.current = null;
  }, []);

  const fitView = useCallback(
    (nodes: GraphNode[], padding = 0.1) => {
      const el = containerRef.current;
      if (!el || nodes.length === 0) return;
      const cw = el.clientWidth;
      const ch = el.clientHeight;
      if (cw === 0 || ch === 0) return;

      let minX = Infinity,
        minY = Infinity,
        maxX = -Infinity,
        maxY = -Infinity;
      for (const n of nodes) {
        if (n.x < minX) minX = n.x;
        if (n.y < minY) minY = n.y;
        if (n.x + NW > maxX) maxX = n.x + NW;
        if (n.y + NH > maxY) maxY = n.y + NH;
      }

      const gw = maxX - minX;
      const gh = maxY - minY;
      if (gw === 0 && gh === 0) {
        setTransform({ x: cw / 2 - minX, y: ch / 2 - minY, k: 1 });
        return;
      }

      const scale = Math.min(
        (cw * (1 - padding * 2)) / gw,
        (ch * (1 - padding * 2)) / gh,
        2,
      );
      const cx = minX + gw / 2;
      const cy = minY + gh / 2;

      setTransform({
        k: scale,
        x: cw / 2 - cx * scale,
        y: ch / 2 - cy * scale,
      });
    },
    [containerRef],
  );

  return { transform, onWheel, onMouseDown, onMouseMove, onMouseUp, fitView };
}

export function FitViewGraph({
  nodes,
  edges,
  bg,
  border,
  accentColor,
  onNodeClick,
}: {
  nodes: GraphNode[];
  edges: GraphEdge[];
  colorMode?: "light" | "dark";
  bg: string;
  surface?: string;
  border: string;
  accentColor?: string;
  onNodeClick?: (crateName: string) => void;
}) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const { transform, onWheel, onMouseDown, onMouseMove, onMouseUp, fitView } =
    usePanZoom(containerRef);

  // Fit on mount and when nodes change
  const nodeKeyRef = useRef("");
  useEffect(() => {
    const key = nodes.map((n) => n.name).join(",");
    if (key !== nodeKeyRef.current) {
      nodeKeyRef.current = key;
      fitView(nodes);
    }
  }, [nodes, fitView]);

  // Fit on container resize
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => fitView(nodes));
    ro.observe(el);
    return () => ro.disconnect();
  }, [nodes, fitView]);

  // Build position lookup for edges
  const posMap = useMemo(() => {
    const m = new Map<string, Point>();
    for (const n of nodes) m.set(n.name, { x: n.x, y: n.y });
    return m;
  }, [nodes]);

  const handleClick = useCallback(
    (e: React.MouseEvent) => {
      if (!onNodeClick) return;
      const target = (e.target as HTMLElement).closest("[data-crate]");
      if (!target) return;
      const name = (target as HTMLElement).dataset.crate;
      if (name) onNodeClick(name);
    },
    [onNodeClick],
  );

  const svgContent = useMemo(() => {
    const edgeEls: React.ReactNode[] = [];
    for (let i = 0; i < edges.length; i++) {
      const edge = edges[i];
      const sp = posMap.get(edge.source);
      const tp = posMap.get(edge.target);
      if (!sp || !tp) continue;
      const x1 = sp.x + NW / 2;
      const y1 = sp.y + NH;
      const x2 = tp.x + NW / 2;
      const y2 = tp.y;
      edgeEls.push(
        <line
          key={`${edge.source}->${edge.target}`}
          x1={x1}
          y1={y1}
          x2={x2}
          y2={y2}
          stroke={edge.color}
          strokeWidth={1.5}
          opacity={0.3}
          markerEnd="url(#arrow)"
        />,
      );
    }

    const nodeEls: React.ReactNode[] = [];
    for (const n of nodes) {
      const hl = n.highlighted && accentColor;
      nodeEls.push(
        <g
          key={n.name}
          data-crate={n.name}
          transform={`translate(${n.x},${n.y})`}
          style={{ cursor: "pointer" }}
        >
          <rect
            width={NW}
            height={NH}
            rx={6}
            fill={n.colors.bg}
            stroke={hl ? accentColor : n.colors.border}
            strokeWidth={hl ? 2.5 : 1.5}
          />
          {!n.external && (
            <text
              x={NW / 2}
              y={14}
              textAnchor="middle"
              fill={n.colors.border}
              fontSize={8}
              fontFamily={MONO}
              letterSpacing={0.8}
            >
              {n.heat}%
            </text>
          )}
          <text
            x={NW / 2}
            y={n.external ? NH / 2 + 4 : NH / 2 + 8}
            textAnchor="middle"
            fill={n.colors.text}
            fontSize={11}
            fontFamily={MONO}
            fontWeight={500}
          >
            {n.external ? "📦 " : ""}
            {n.name}
          </text>
        </g>,
      );
    }

    return { edgeEls, nodeEls };
  }, [nodes, edges, posMap, accentColor]);

  return (
    <div
      ref={containerRef}
      onWheel={onWheel}
      onMouseDown={onMouseDown}
      onMouseMove={onMouseMove}
      onMouseUp={onMouseUp}
      onMouseLeave={onMouseUp}
      onClick={handleClick}
      style={{
        width: "100%",
        height: "100%",
        overflow: "hidden",
        background: bg,
        border: `1px solid ${border}`,
        borderRadius: 4,
        cursor: "grab",
        userSelect: "none",
      }}
    >
      <svg width="100%" height="100%" style={{ display: "block" }}>
        <defs>
          <marker
            id="arrow"
            viewBox="0 0 10 10"
            refX={10}
            refY={5}
            markerWidth={8}
            markerHeight={8}
            orient="auto-start-reverse"
          >
            <path d="M 0 0 L 10 5 L 0 10 z" fill="#888" opacity={0.5} />
          </marker>
        </defs>
        <g
          transform={`translate(${transform.x},${transform.y}) scale(${transform.k})`}
        >
          {svgContent.edgeEls}
          {svgContent.nodeEls}
        </g>
      </svg>
    </div>
  );
}
