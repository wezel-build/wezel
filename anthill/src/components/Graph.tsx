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

  // Kahn's BFS: layer by dependant count (things that depend on me).
  // Layer 0 = nodes nothing depends on (top-level consumers).
  // Edges always flow downward.
  const inDegree = new Map<string, number>();
  for (const c of items) inDegree.set(c.name, 0);
  for (const c of items) {
    for (const dep of c.deps) {
      if (nameSet.has(dep)) {
        inDegree.set(dep, (inDegree.get(dep) ?? 0) + 1);
      }
    }
  }

  const layers: string[][] = [];
  const placed = new Set<string>();
  let queue = items
    .filter((c) => inDegree.get(c.name) === 0)
    .map((c) => c.name);

  while (queue.length > 0) {
    // Workspace crates first within each layer
    queue.sort((a, b) => {
      const ae = nameToItem.get(a)?.external ? 1 : 0;
      const be = nameToItem.get(b)?.external ? 1 : 0;
      return ae - be;
    });
    layers.push(queue);
    for (const name of queue) placed.add(name);

    const next: string[] = [];
    for (const name of queue) {
      const node = nameToItem.get(name)!;
      for (const dep of node.deps) {
        if (!nameSet.has(dep) || placed.has(dep)) continue;
        const newDeg = (inDegree.get(dep) ?? 1) - 1;
        inDegree.set(dep, newDeg);
        if (newDeg === 0) next.push(dep);
      }
    }
    queue = next;
  }

  // Barycenter crossing minimization: reorder nodes within each layer
  // based on the average position of their connected nodes in the adjacent layer.
  // Run a few passes (down then up) for convergence.
  const layerIndex = new Map<string, number>();
  for (let li = 0; li < layers.length; li++) {
    for (let ni = 0; ni < layers[li].length; ni++) {
      layerIndex.set(layers[li][ni], ni);
    }
  }

  // Build both directions: dependants (up) and deps (down)
  const depsOf = new Map<string, string[]>();
  const dependantsOf = new Map<string, string[]>();
  for (const c of items) {
    const filtered = c.deps.filter((d) => nameSet.has(d));
    depsOf.set(c.name, filtered);
    for (const dep of filtered) {
      let arr = dependantsOf.get(dep);
      if (!arr) {
        arr = [];
        dependantsOf.set(dep, arr);
      }
      arr.push(c.name);
    }
  }

  function barycenter(
    layer: string[],
    neighborLayer: string[],
    getNeighbors: (name: string) => string[],
  ) {
    const posInNeighbor = new Map<string, number>();
    for (let i = 0; i < neighborLayer.length; i++) {
      posInNeighbor.set(neighborLayer[i], i);
    }

    const bary = new Map<string, number>();
    for (const name of layer) {
      const neighbors = getNeighbors(name).filter((n) => posInNeighbor.has(n));
      if (neighbors.length === 0) {
        // Keep current position as fallback
        bary.set(name, layerIndex.get(name) ?? 0);
      } else {
        const avg =
          neighbors.reduce((s, n) => s + posInNeighbor.get(n)!, 0) /
          neighbors.length;
        bary.set(name, avg);
      }
    }

    layer.sort((a, b) => bary.get(a)! - bary.get(b)!);
    // Update layerIndex
    for (let i = 0; i < layer.length; i++) {
      layerIndex.set(layer[i], i);
    }
  }

  const SWEEPS = 4;
  for (let sweep = 0; sweep < SWEEPS; sweep++) {
    // Down sweep: reorder each layer based on dependants in the layer above
    for (let li = 1; li < layers.length; li++) {
      barycenter(
        layers[li],
        layers[li - 1],
        (name) => dependantsOf.get(name) ?? [],
      );
    }
    // Up sweep: reorder each layer based on deps in the layer below
    for (let li = layers.length - 2; li >= 0; li--) {
      barycenter(layers[li], layers[li + 1], (name) => depsOf.get(name) ?? []);
    }
  }

  // Handle cycles: any remaining nodes go in a final layer
  if (placed.size < items.length) {
    const remaining = items
      .filter((c) => !placed.has(c.name))
      .map((c) => c.name);
    layers.push(remaining);
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

  // Initial grid positions
  const posX = new Map<string, number>();
  const posY = new Map<string, number>();

  for (let ly = 0; ly < layers.length; ly++) {
    const layer = layers[ly];
    const w = layer.length * NW + (layer.length - 1) * GX;
    for (let ci = 0; ci < layer.length; ci++) {
      posX.set(layer[ci], -w / 2 + ci * (NW + GX));
      posY.set(layer[ci], ly * (NH + GY));
    }
  }

  // Node positioning: shift each node toward the average x of its neighbors
  // (deps below + dependants above) while respecting ordering constraints.
  const POS_SWEEPS = 8;
  for (let pass = 0; pass < POS_SWEEPS; pass++) {
    // Down sweep
    for (let li = 1; li < layers.length; li++) {
      nudgeLayer(layers[li], (name) => dependantsOf.get(name) ?? []);
    }
    // Up sweep
    for (let li = layers.length - 2; li >= 0; li--) {
      nudgeLayer(layers[li], (name) => depsOf.get(name) ?? []);
    }
  }

  function nudgeLayer(
    layer: string[],
    getNeighbors: (name: string) => string[],
  ) {
    if (layer.length === 0) return;

    // Compute ideal x for each node (average of neighbor center x)
    const ideal = new Map<string, number>();
    for (const name of layer) {
      const nbrs = getNeighbors(name).filter((n) => posX.has(n));
      if (nbrs.length > 0) {
        const avg =
          nbrs.reduce((s, n) => s + posX.get(n)! + NW / 2, 0) / nbrs.length -
          NW / 2;
        ideal.set(name, avg);
      } else {
        ideal.set(name, posX.get(name) ?? 0);
      }
    }

    // Nudge left-to-right: each node goes to its ideal x but can't overlap previous
    let left = -Infinity;
    for (const name of layer) {
      const x = Math.max(ideal.get(name)!, left);
      posX.set(name, x);
      left = x + NW + GX;
    }

    // Nudge right-to-left: same but from the right, then average both passes
    const rightPass = new Map<string, number>();
    let right = Infinity;
    for (let i = layer.length - 1; i >= 0; i--) {
      const name = layer[i];
      const x = Math.min(ideal.get(name)!, right - NW);
      rightPass.set(name, x);
      right = x - GX;
    }

    // Average both passes for balanced result
    for (const name of layer) {
      posX.set(name, (posX.get(name)! + rightPass.get(name)!) / 2);
    }

    // Re-center the layer around x=0
    let sumX = 0;
    for (const name of layer) sumX += posX.get(name)!;
    const center = sumX / layer.length + NW / 2;
    for (const name of layer) {
      posX.set(name, posX.get(name)! - center);
    }
  }

  // Build final node list
  const nodePositions = new Map<string, { x: number; y: number }>();
  const nodes: GraphNode[] = [];

  for (let ly = 0; ly < layers.length; ly++) {
    const layer = layers[ly];
    for (const name of layer) {
      const item = nameToItem.get(name)!;
      const colors = getCachedColor(item.heat);
      const x = posX.get(name)!;
      const y = posY.get(name)!;
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
  const [hoveredCrate, setHoveredCrate] = useState<string | null>(null);

  // Build reverse-dep map: for each crate, which crates depend on it?
  // Also collect all ancestors (transitive dependants) for hover highlighting.
  const dependantsMap = useMemo(() => {
    const rev = new Map<string, Set<string>>();
    for (const e of edges) {
      let s = rev.get(e.target);
      if (!s) {
        s = new Set();
        rev.set(e.target, s);
      }
      s.add(e.source);
    }
    return rev;
  }, [edges]);

  // On hover: the hovered node + all transitive dependants
  const hoverSet = useMemo(() => {
    if (!hoveredCrate) return null;
    const set = new Set<string>();
    const queue = [hoveredCrate];
    while (queue.length > 0) {
      const name = queue.pop()!;
      if (set.has(name)) continue;
      set.add(name);
      const parents = dependantsMap.get(name);
      if (parents) {
        for (const p of parents) queue.push(p);
      }
    }
    return set;
  }, [hoveredCrate, dependantsMap]);

  // Edges that connect hovered nodes
  const hoverEdgeSet = useMemo(() => {
    if (!hoverSet) return null;
    const set = new Set<string>();
    for (const e of edges) {
      if (hoverSet.has(e.source) && hoverSet.has(e.target)) {
        set.add(`${e.source}->${e.target}`);
      }
    }
    return set;
  }, [hoverSet, edges]);

  const handleMouseEnter = useCallback((e: React.MouseEvent) => {
    const target = (e.target as HTMLElement).closest("[data-crate]");
    if (target) {
      setHoveredCrate((target as HTMLElement).dataset.crate ?? null);
    }
  }, []);

  const handleMouseLeave = useCallback((e: React.MouseEvent) => {
    const target = (e.target as HTMLElement).closest("[data-crate]");
    if (target) {
      setHoveredCrate(null);
    }
  }, []);

  const containerRef = useRef<HTMLDivElement | null>(null);
  const { transform, onWheel, onMouseDown, onMouseMove, onMouseUp, fitView } =
    usePanZoom(containerRef);

  // Keep a ref to the latest nodes so resize/fitView don't use stale data
  const nodesRef = useRef(nodes);
  nodesRef.current = nodes;

  // Only fit when the graph topology actually changes (set of node names)
  const nodeKeyRef = useRef("");
  useEffect(() => {
    const key = nodes.map((n) => n.name).join(",");
    if (key !== nodeKeyRef.current) {
      nodeKeyRef.current = key;
      fitView(nodes);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [nodes.length, fitView]);

  // Fit on container resize (stable effect, reads ref)
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => fitView(nodesRef.current));
    ro.observe(el);
    return () => ro.disconnect();
  }, [fitView]);

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
    const dimmed = hoverSet !== null;

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
      const edgeKey = `${edge.source}->${edge.target}`;
      const active = !dimmed || hoverEdgeSet?.has(edgeKey);
      edgeEls.push(
        <line
          key={edgeKey}
          x1={x1}
          y1={y1}
          x2={x2}
          y2={y2}
          stroke={active ? edge.color : edge.color}
          strokeWidth={active && dimmed ? 2 : 1.5}
          opacity={active ? (dimmed ? 0.6 : 0.3) : 0.06}
          markerEnd="url(#arrow)"
        />,
      );
    }

    const nodeEls: React.ReactNode[] = [];
    for (const n of nodes) {
      const hl = n.highlighted && accentColor;
      const active = !dimmed || hoverSet?.has(n.name);
      const nodeOpacity = active ? 1 : 0.15;
      nodeEls.push(
        <g
          key={n.name}
          data-crate={n.name}
          transform={`translate(${n.x},${n.y})`}
          style={{ cursor: "pointer", opacity: nodeOpacity }}
        >
          <rect
            width={NW}
            height={NH}
            rx={6}
            fill={n.colors.bg}
            stroke={hl ? accentColor : n.colors.border}
            strokeWidth={hl ? 2.5 : 1.5}
          />
          {n.external && (
            <>
              <rect
                width={NW}
                height={NH}
                rx={6}
                fill="url(#ext-checker)"
                style={{ color: n.colors.border }}
              />
              <text x={NW - 6} y={12} textAnchor="end" fontSize={10}>
                📦
              </text>
            </>
          )}
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
          <text
            x={NW / 2}
            y={NH / 2 + 8}
            textAnchor="middle"
            fill={n.colors.text}
            fontSize={11}
            fontFamily={MONO}
            fontWeight={500}
          >
            {n.name}
          </text>
        </g>,
      );
    }

    return { edgeEls, nodeEls };
  }, [nodes, edges, posMap, accentColor, hoverSet, hoverEdgeSet]);

  return (
    <div
      ref={containerRef}
      onWheel={onWheel}
      onMouseDown={onMouseDown}
      onMouseMove={onMouseMove}
      onMouseUp={onMouseUp}
      onMouseLeave={(e) => {
        onMouseUp();
        setHoveredCrate(null);
        void e;
      }}
      onClick={handleClick}
      onMouseOver={handleMouseEnter}
      onMouseOut={handleMouseLeave}
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
          <pattern
            id="ext-checker"
            width={12}
            height={12}
            patternUnits="userSpaceOnUse"
          >
            <rect width={12} height={12} fill="transparent" />
            <rect width={6} height={6} fill="currentColor" opacity={0.1} />
            <rect
              x={6}
              y={6}
              width={6}
              height={6}
              fill="currentColor"
              opacity={0.1}
            />
          </pattern>
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
