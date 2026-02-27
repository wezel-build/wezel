import { useEffect } from "react";
import {
  ReactFlow,
  Background,
  Controls,
  Handle,
  Position,
  BackgroundVariant,
  MarkerType,
  useReactFlow,
  ReactFlowProvider,
  type Node,
  type Edge,
  type NodeProps,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";

import { MONO } from "../lib/format";
import type { CrateTopo } from "../lib/data";
import type { HeatFn } from "../lib/theme";

// ── Graph layout ─────────────────────────────────────────────────────────────

interface LayoutNode {
  name: string;
  deps: string[];
  heat: number;
}

export function layoutGraph(
  topo: CrateTopo[],
  heat: Record<string, number>,
  heatColor: HeatFn,
): { nodes: Node[]; edges: Edge[] } {
  const items: LayoutNode[] = topo.map((c) => ({
    ...c,
    heat: heat[c.name] ?? 0,
  }));

  const nameToIdx = new Map<string, number>();
  items.forEach((c, i) => nameToIdx.set(c.name, i));

  const depths = new Map<string, number>();
  function getDepth(name: string): number {
    if (depths.has(name)) return depths.get(name)!;
    const node = items.find((c) => c.name === name);
    if (!node || node.deps.length === 0) {
      depths.set(name, 0);
      return 0;
    }
    const d =
      1 +
      Math.max(
        ...node.deps.filter((d) => nameToIdx.has(d)).map((d) => getDepth(d)),
      );
    depths.set(name, d);
    return d;
  }
  items.forEach((c) => getDepth(c.name));

  const maxDepth = Math.max(...Array.from(depths.values()), 0);
  const layers: string[][] = Array.from({ length: maxDepth + 1 }, () => []);
  items.forEach((c) => {
    layers[maxDepth - (depths.get(c.name) ?? 0)].push(c.name);
  });

  const NW = 150,
    NH = 44,
    GX = 32,
    GY = 72;
  const nodes: Node[] = [];
  const edges: Edge[] = [];

  layers.forEach((layer, ly) => {
    const w = layer.length * NW + (layer.length - 1) * GX;
    layer.forEach((name, ci) => {
      const item = items.find((c) => c.name === name)!;
      const colors = heatColor(item.heat);
      nodes.push({
        id: name,
        type: "crate",
        position: { x: -w / 2 + ci * (NW + GX), y: ly * (NH + GY) },
        data: { label: name, heat: item.heat, colors },
      });
    });
  });

  items.forEach((crate) => {
    crate.deps.forEach((dep) => {
      if (nameToIdx.has(dep)) {
        const col = heatColor(crate.heat);
        edges.push({
          id: `${crate.name}->${dep}`,
          source: crate.name,
          target: dep,
          style: { stroke: col.border, strokeWidth: 1.5, opacity: 0.45 },
          markerEnd: {
            type: MarkerType.ArrowClosed,
            color: col.border,
            width: 12,
            height: 12,
          },
        });
      }
    });
  });

  return { nodes, edges };
}

// ── ReactFlow crate node ─────────────────────────────────────────────────────

function CrateNodeComponent({ data }: NodeProps) {
  const d = data as {
    label: string;
    heat: number;
    colors: { border: string; bg: string; text: string };
  };
  return (
    <div
      style={{
        background: d.colors.bg,
        border: `1.5px solid ${d.colors.border}`,
        borderRadius: 6,
        padding: "4px 10px",
        color: d.colors.text,
        fontSize: 11,
        fontFamily: MONO,
        fontWeight: 500,
        minWidth: 100,
        textAlign: "center",
        boxShadow: `0 0 8px ${d.colors.border}22`,
      }}
    >
      <Handle
        type="target"
        position={Position.Top}
        style={{
          background: d.colors.border,
          width: 5,
          height: 5,
          border: "none",
        }}
      />
      <div
        style={{
          fontSize: 8,
          color: d.colors.border,
          letterSpacing: 0.8,
          marginBottom: 1,
        }}
      >
        {d.heat}%
      </div>
      <div>{d.label}</div>
      <Handle
        type="source"
        position={Position.Bottom}
        style={{
          background: d.colors.border,
          width: 5,
          height: 5,
          border: "none",
        }}
      />
    </div>
  );
}

export const nodeTypes = { crate: CrateNodeComponent };

// ── Graph wrapper that re-fits on resize ─────────────────────────────────────

function FitViewOnResize() {
  const { fitView } = useReactFlow();

  useEffect(() => {
    const el = document.querySelector(".react-flow") as HTMLElement | null;
    if (!el) return;
    const ro = new ResizeObserver(() => {
      fitView({ padding: 0.25, duration: 150 });
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, [fitView]);

  return null;
}

export function FitViewGraph({
  nodes,
  edges,
  colorMode,
  bg,
  surface,
  border,
}: {
  nodes: Node[];
  edges: Edge[];
  colorMode: "light" | "dark";
  bg: string;
  surface: string;
  border: string;
}) {
  return (
    <ReactFlowProvider>
      <ReactFlow
        nodes={nodes}
        edges={edges}
        nodeTypes={nodeTypes}
        fitView
        fitViewOptions={{ padding: 0.25 }}
        colorMode={colorMode}
        minZoom={0.3}
        maxZoom={2}
        proOptions={{ hideAttribution: true }}
      >
        <FitViewOnResize />
        <Background
          variant={BackgroundVariant.Dots}
          gap={16}
          size={1}
          color={bg}
        />
        <Controls
          style={{
            background: surface,
            borderRadius: 4,
            border: `1px solid ${border}`,
          }}
        />
      </ReactFlow>
    </ReactFlowProvider>
  );
}