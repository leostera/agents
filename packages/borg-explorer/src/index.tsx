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
  const centerX = 600;
  const centerY = 380;
  const radius = Math.min(220 + count * 10, 420);

  const nodes: Node<GraphNodeData>[] = entities.map((entity, index) => {
    const angle = (index / count) * Math.PI * 2;
    return {
      id: entity.entity_id,
      data: {
        label: entity.label || entity.entity_id,
        type: entity.entity_type,
      },
      position: {
        x: centerX + Math.cos(angle) * radius,
        y: centerY + Math.sin(angle) * radius,
      },
      draggable: false,
    };
  });

  const ids = new Set(nodes.map((node) => node.id));
  const edgeSet = new Set<string>();
  const edges: Edge[] = [];

  const mergedEdges = [...explorerEdges, ...deriveEdgesFromEntities(entities)];

  for (const edge of mergedEdges) {
    if (
      !ids.has(edge.source) ||
      !ids.has(edge.target) ||
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

  async function runSearch(nextQuery = query) {
    setLoading(true);
    setError("");
    try {
      const graphData = await fetchGraph(nextQuery.trim() || "borg", limit);
      setEntities(graphData.entities);
      setExplorerEdges(graphData.edges);
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

  const graph = useMemo(
    () => buildGraph(entities, explorerEdges),
    [entities, explorerEdges]
  );
  const flowNodes = useMemo(
    () =>
      graph.nodes.map((node) => ({
        ...node,
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
    [graph.nodes]
  );

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
              nodesDraggable={false}
              nodesConnectable={false}
              elementsSelectable
              panOnDrag
              panOnScroll
              zoomOnScroll
              zoomOnPinch
              attributionPosition="bottom-left"
              onNodeClick={(_, node) => {
                window.location.href = `/memory/entity/${node.id}`;
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
