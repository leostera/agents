import { createBorgApiClient, MemoryExplorerEdge } from "@borg/api";
import React, { useMemo, useState } from "react";
import ReactFlow, {
  Background,
  Controls,
  Edge,
  MarkerType,
  MiniMap,
  Node,
  Panel,
  ReactFlowInstance,
} from "reactflow";
import "reactflow/dist/style.css";
import "./styles.css";

type MemoryEntity = {
  entity_id: string;
  entity_type: string;
  label: string;
  props?: Record<string, unknown>;
};

type GraphNodeData = {
  label: string;
  type: string;
};

type MemoryExplorerProps = {
  className?: string;
  initialQuery?: string;
};

const borgApi = createBorgApiClient();

function extractUriRefs(
  value: unknown,
  into: Map<string, Set<string>>,
  path = ""
) {
  if (typeof value === "string") {
    if (value.includes(":")) {
      const labels = into.get(value) ?? new Set<string>();
      labels.add(path || "ref");
      into.set(value, labels);
    }
    return;
  }

  if (Array.isArray(value)) {
    for (let index = 0; index < value.length; index += 1) {
      extractUriRefs(value[index], into, `${path}[${index}]`);
    }
    return;
  }

  if (value && typeof value === "object") {
    for (const [key, nested] of Object.entries(
      value as Record<string, unknown>
    )) {
      const nestedPath = path ? `${path}.${key}` : key;
      extractUriRefs(nested, into, nestedPath);
    }
  }
}

function deriveEdgesFromEntities(
  entities: MemoryEntity[]
): MemoryExplorerEdge[] {
  const entityIds = new Set(entities.map((entity) => entity.entity_id));
  const edges: MemoryExplorerEdge[] = [];

  for (const entity of entities) {
    const refs = new Map<string, Set<string>>();
    extractUriRefs(entity.props ?? {}, refs);
    for (const [target, labels] of refs.entries()) {
      if (!entityIds.has(target) || target === entity.entity_id) continue;
      const relation =
        labels.size <= 2
          ? Array.from(labels).join(", ")
          : `${Array.from(labels).slice(0, 2).join(", ")} +${labels.size - 2}`;
      edges.push({
        source: entity.entity_id,
        target,
        relation: relation || "ref",
      });
    }
  }

  return edges;
}

function buildGraph(
  entities: MemoryEntity[],
  explorerEdges: MemoryExplorerEdge[]
): { nodes: Node<GraphNodeData>[]; edges: Edge[] } {
  const count = Math.max(entities.length, 1);
  const ids = new Set(entities.map((entity) => entity.entity_id));
  const mergedEdges = [...explorerEdges, ...deriveEdgesFromEntities(entities)];
  const neighbors = new Map<string, Set<string>>();
  for (const edge of mergedEdges) {
    if (
      !ids.has(edge.source) ||
      !ids.has(edge.target) ||
      edge.source === edge.target
    ) {
      continue;
    }
    const sourceSet = neighbors.get(edge.source) ?? new Set<string>();
    sourceSet.add(edge.target);
    neighbors.set(edge.source, sourceSet);

    const targetSet = neighbors.get(edge.target) ?? new Set<string>();
    targetSet.add(edge.source);
    neighbors.set(edge.target, targetSet);
  }

  const horizontalGap = 220;
  const verticalGap = 150;
  const originX = 120;
  const originY = 120;
  const layerIndexById = new Map<string, number>();
  const orderInLayerById = new Map<string, number>();
  const layerCounts = new Map<number, number>();

  const unvisited = new Set(entities.map((entity) => entity.entity_id));
  const components: string[][] = [];
  const queue: string[] = [];
  while (unvisited.size > 0) {
    const root = [...unvisited].sort((a, b) => {
      const degreeA = neighbors.get(a)?.size ?? 0;
      const degreeB = neighbors.get(b)?.size ?? 0;
      if (degreeA !== degreeB) return degreeB - degreeA;
      return a.localeCompare(b);
    })[0];
    const component: string[] = [];
    queue.push(root);
    unvisited.delete(root);
    while (queue.length > 0) {
      const current = queue.shift()!;
      component.push(current);
      const next = [...(neighbors.get(current) ?? new Set<string>())].sort();
      for (const candidate of next) {
        if (unvisited.has(candidate)) {
          unvisited.delete(candidate);
          queue.push(candidate);
        }
      }
    }
    components.push(component);
  }

  let globalLayerOffset = 0;
  for (const component of components) {
    const root = [...component].sort((a, b) => {
      const degreeA = neighbors.get(a)?.size ?? 0;
      const degreeB = neighbors.get(b)?.size ?? 0;
      if (degreeA !== degreeB) return degreeB - degreeA;
      return a.localeCompare(b);
    })[0];

    const localDepth = new Map<string, number>();
    const localVisited = new Set<string>();
    const localQueue: string[] = [root];
    localDepth.set(root, 0);
    localVisited.add(root);

    while (localQueue.length > 0) {
      const current = localQueue.shift()!;
      const currentDepth = localDepth.get(current) ?? 0;
      const next = [...(neighbors.get(current) ?? new Set<string>())].sort();
      for (const candidate of next) {
        if (localVisited.has(candidate) || !component.includes(candidate))
          continue;
        localVisited.add(candidate);
        localDepth.set(candidate, currentDepth + 1);
        localQueue.push(candidate);
      }
    }

    for (const id of component) {
      if (!localDepth.has(id)) {
        localDepth.set(id, 0);
      }
    }

    const byDepth = new Map<number, string[]>();
    for (const id of component) {
      const depth = localDepth.get(id) ?? 0;
      const items = byDepth.get(depth) ?? [];
      items.push(id);
      byDepth.set(depth, items);
    }
    const maxDepth = Math.max(...byDepth.keys());
    for (let depth = 0; depth <= maxDepth; depth += 1) {
      const idsAtDepth = (byDepth.get(depth) ?? []).sort((a, b) => {
        const degreeA = neighbors.get(a)?.size ?? 0;
        const degreeB = neighbors.get(b)?.size ?? 0;
        if (degreeA !== degreeB) return degreeB - degreeA;
        return a.localeCompare(b);
      });
      const globalDepth = globalLayerOffset + depth;
      layerCounts.set(globalDepth, idsAtDepth.length);
      idsAtDepth.forEach((id, index) => {
        layerIndexById.set(id, globalDepth);
        orderInLayerById.set(id, index);
      });
    }
    globalLayerOffset += maxDepth + 2;
  }

  const nodes: Node<GraphNodeData>[] = entities.map((entity, index) => {
    const layer = layerIndexById.get(entity.entity_id) ?? 0;
    const order = orderInLayerById.get(entity.entity_id) ?? index;
    const layerCount = layerCounts.get(layer) ?? 1;
    const centeredOffset = order - (layerCount - 1) / 2;
    const props = entity.props ?? {};
    const propLabel =
      (typeof props.label === "string" && props.label) ||
      (typeof props["borg:field:label"] === "string" &&
        (props["borg:field:label"] as string)) ||
      (typeof props["rdfs:label"] === "string" &&
        (props["rdfs:label"] as string)) ||
      (typeof props["schema:name"] === "string" &&
        (props["schema:name"] as string));
    return {
      id: entity.entity_id,
      data: {
        label: propLabel || entity.label || entity.entity_id,
        type: entity.entity_type,
      },
      position: {
        x: originX + centeredOffset * horizontalGap,
        y: originY + layer * verticalGap,
      },
      draggable: false,
    };
  });

  const nodeIds = new Set(nodes.map((node) => node.id));
  const edgeSet = new Set<string>();
  const edges: Edge[] = [];

  for (const edge of mergedEdges) {
    if (
      !nodeIds.has(edge.source) ||
      !nodeIds.has(edge.target) ||
      edge.source === edge.target
    )
      continue;
    const edgeKey = `${edge.source}=>${edge.relation}=>${edge.target}`;
    if (edgeSet.has(edgeKey)) continue;
    edgeSet.add(edgeKey);
    edges.push({
      id: edgeKey,
      source: edge.source,
      target: edge.target,
      label: edge.relation,
      markerEnd: { type: MarkerType.ArrowClosed, width: 14, height: 14 },
      style: { stroke: "rgba(148, 163, 184, 0.5)" },
      labelStyle: {
        fill: "rgba(186, 230, 253, 0.95)",
        fontSize: 10,
        fontFamily:
          "ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace",
      },
      labelBgPadding: [4, 2],
      labelBgBorderRadius: 4,
      labelBgStyle: { fill: "rgba(2, 6, 23, 0.7)" },
    });
  }

  return { nodes, edges };
}

async function fetchGraph(
  query: string,
  limit: number
): Promise<{ entities: MemoryEntity[]; edges: MemoryExplorerEdge[] }> {
  return await borgApi.exploreMemory({ query, limit, maxNodes: limit });
}

export function MemoryExplorer({
  className,
  initialQuery = "borg",
}: MemoryExplorerProps) {
  const [query, setQuery] = useState(initialQuery);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [entities, setEntities] = useState<MemoryEntity[]>([]);
  const [explorerEdges, setExplorerEdges] = useState<MemoryExplorerEdge[]>([]);
  const [limit, setLimit] = useState(300);
  const [flow, setFlow] = useState<ReactFlowInstance | null>(null);
  const [focusEntityId, setFocusEntityId] = useState<string | null>(null);
  const [selectedEntityId, setSelectedEntityId] = useState<string | null>(null);
  const [manualPositions, setManualPositions] = useState<
    Record<string, { x: number; y: number }>
  >({});

  async function runSearch(nextQuery = query) {
    setLoading(true);
    setError("");
    try {
      const graphData = await fetchGraph(nextQuery.trim() || "borg", limit);
      setEntities(graphData.entities);
      setExplorerEdges(graphData.edges);
      setSelectedEntityId(null);
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Failed to search memory");
      setEntities([]);
      setExplorerEdges([]);
    } finally {
      setLoading(false);
    }
  }

  React.useEffect(() => {
    runSearch(initialQuery);
    // initial load only
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const filteredGraphData = useMemo(() => {
    if (!selectedEntityId) {
      return { entities, edges: explorerEdges };
    }

    const entityById = new Map(
      entities.map((entity) => [entity.entity_id, entity])
    );
    const adjacency = new Map<string, Set<string>>();
    for (const edge of explorerEdges) {
      const from = adjacency.get(edge.source) ?? new Set<string>();
      from.add(edge.target);
      adjacency.set(edge.source, from);
      const to = adjacency.get(edge.target) ?? new Set<string>();
      to.add(edge.source);
      adjacency.set(edge.target, to);
    }

    const connected = new Set<string>();
    const queue = [selectedEntityId];
    while (queue.length > 0) {
      const current = queue.shift()!;
      if (connected.has(current)) continue;
      connected.add(current);
      for (const neighbor of adjacency.get(current) ?? []) {
        if (!connected.has(neighbor)) {
          queue.push(neighbor);
        }
      }
    }

    const filteredEntities = [...connected]
      .map((id) => entityById.get(id))
      .filter((entity): entity is MemoryEntity => Boolean(entity));
    const filteredEdges = explorerEdges.filter(
      (edge) => connected.has(edge.source) && connected.has(edge.target)
    );

    return { entities: filteredEntities, edges: filteredEdges };
  }, [entities, explorerEdges, selectedEntityId]);

  const graph = useMemo(
    () => buildGraph(filteredGraphData.entities, filteredGraphData.edges),
    [filteredGraphData]
  );
  const flowNodes = useMemo(
    () =>
      graph.nodes.map((node) => ({
        ...node,
        position: manualPositions[node.id] ?? node.position,
        style: {
          border: "1px solid rgba(148, 163, 184, 0.35)",
          background: "rgba(15, 23, 42, 0.86)",
          color: "#e2e8f0",
          borderRadius: 10,
          fontSize: 12,
          padding: "6px 8px",
          minWidth: 130,
        },
      })),
    [graph.nodes, manualPositions]
  );

  React.useEffect(() => {
    if (!focusEntityId || !flow) return;
    const node = flowNodes.find((candidate) => candidate.id === focusEntityId);
    if (!node) return;
    flow.setCenter(node.position.x, node.position.y, {
      zoom: 1.1,
      duration: 350,
    });
    setFocusEntityId(null);
  }, [focusEntityId, flow, flowNodes]);

  return (
    <section className={`explorer-root ${className ?? ""}`.trim()}>
      {error ? <p className="explorer-empty">{error}</p> : null}

      <div className="explorer-canvas">
        {graph.nodes.length === 0 ? (
          <p className="explorer-empty">
            No memory entities found for this query.
          </p>
        ) : (
          <div className="explorer-flow">
            <ReactFlow
              nodes={flowNodes}
              edges={graph.edges}
              fitView
              minZoom={0.3}
              maxZoom={2.2}
              nodesDraggable
              nodesConnectable={false}
              elementsSelectable
              panOnDrag
              panOnScroll
              zoomOnScroll
              zoomOnPinch
              attributionPosition="bottom-left"
              onInit={setFlow}
              onNodeClick={(_, node) => {
                setFocusEntityId(node.id);
                setSelectedEntityId((current) =>
                  current === node.id ? null : node.id
                );
              }}
              onNodeDrag={(_, node) => {
                setManualPositions((current) => ({
                  ...current,
                  [node.id]: { x: node.position.x, y: node.position.y },
                }));
              }}
            >
              <Background color="rgba(148, 163, 184, 0.2)" gap={24} />
              <MiniMap
                pannable
                zoomable
                nodeStrokeColor="rgba(148, 163, 184, 0.6)"
                nodeColor="rgba(15, 23, 42, 0.85)"
                maskColor="rgba(2, 6, 23, 0.65)"
              />
              <Controls showInteractive={false} />
              <Panel position="top-left" className="explorer-flow-panel">
                <div className="explorer-toolbar explorer-toolbar--inside">
                  <input
                    className="explorer-input"
                    value={query}
                    onChange={(event) => setQuery(event.currentTarget.value)}
                    placeholder="Search memory entities"
                    aria-label="Search memory entities"
                  />
                  <input
                    className="explorer-input"
                    value={limit}
                    onChange={(event) =>
                      setLimit(
                        Math.max(10, Number(event.currentTarget.value) || 300)
                      )
                    }
                    type="number"
                    min={10}
                    max={2000}
                    aria-label="Explorer limit"
                    title="Max entities to include"
                  />
                  <button
                    className="explorer-button"
                    onClick={() => runSearch()}
                    disabled={loading}
                  >
                    {loading ? "Loading..." : "Search"}
                  </button>
                </div>
              </Panel>
              <Panel position="top-right" className="explorer-flow-panel">
                {graph.nodes.length} nodes, {graph.edges.length} edges
              </Panel>
            </ReactFlow>
          </div>
        )}
      </div>
    </section>
  );
}
