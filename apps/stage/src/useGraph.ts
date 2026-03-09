import React from "react";
import { type Edge, type Node } from "reactflow";
import {
  ActorSummary,
  MailboxMessage,
  PortSummary,
  ProviderInfo,
} from "./types";
import {
  colorForProvider,
  extractSenderActorIdFromPayload,
  extractTargetActorIdFromPayload,
  extractToolName,
  isActorEventMessage,
  normalizeToolGroup,
} from "./utils";

export function useGraph({
  actors,
  ports,
  providers,
  actorGraphMessages,
  selectedActorId,
  selectedPortId,
  selectedProviderId,
}: {
  actors: ActorSummary[];
  ports: PortSummary[];
  providers: ProviderInfo[];
  actorGraphMessages: Record<string, MailboxMessage[]>;
  selectedActorId: string | null;
  selectedPortId: string | null;
  selectedProviderId: string | null;
}) {
  return React.useMemo(() => {
    const allNodes: Node[] = [];
    const allEdges: Edge[] = [];

    const actorIds = new Set(actors.map((a) => a.id));
    const actorById = new Map(actors.map((a) => [a.id, a]));

    const actorDepth = new Map<string, number>();
    const actorToolGroups = new Map<string, Set<string>>();
    const toolGroups = new Set<string>();
    const actorPairs = new Map<string, [string, string]>();

    for (const actor of actors) {
      actorDepth.set(actor.id, 0);
    }

    for (const [actorId, messages] of Object.entries(actorGraphMessages)) {
      const currentDepth = actorDepth.get(actorId) ?? 0;
      const groups = new Set<string>();

      for (const message of messages) {
        if (isActorEventMessage(message)) continue;

        const toolName = extractToolName(message.payload);
        if (toolName) {
          const group = normalizeToolGroup(toolName);
          groups.add(group);
          toolGroups.add(group);
        }

        const targetActorId = extractTargetActorIdFromPayload(message.payload);
        if (
          targetActorId &&
          actorIds.has(targetActorId) &&
          targetActorId !== actorId
        ) {
          const pairKey = [actorId, targetActorId].sort().join("<->");
          actorPairs.set(pairKey, [actorId, targetActorId]);

          const targetDepth = actorDepth.get(targetActorId) ?? 0;
          if (targetDepth <= currentDepth) {
            actorDepth.set(targetActorId, currentDepth + 1);
          }
        }

        const senderActorId = extractSenderActorIdFromPayload(message.payload);
        if (
          senderActorId &&
          actorIds.has(senderActorId) &&
          senderActorId !== actorId
        ) {
          const pairKey = [actorId, senderActorId].sort().join("<->");
          actorPairs.set(pairKey, [senderActorId, actorId]);

          const senderDepth = actorDepth.get(senderActorId) ?? 0;
          if (currentDepth <= senderDepth) {
            actorDepth.set(actorId, senderDepth + 1);
          }
        }
      }
      actorToolGroups.set(actorId, groups);
    }

    const orderedActorsByDepth = new Map<number, string[]>();
    const orderedDepths = Array.from(new Set(actorDepth.values())).sort(
      (a, b) => a - b
    );
    for (const [id, depth] of actorDepth.entries()) {
      const list = orderedActorsByDepth.get(depth) ?? [];
      list.push(id);
      orderedActorsByDepth.set(depth, list);
    }

    const PROVIDER_X_GAP = 220;
    const PROVIDER_LAYER_Y = -150;
    const sortedProviders = [...providers].sort((a, b) =>
      a.provider.localeCompare(b.provider)
    );
    const providerRowWidth = Math.max(
      0,
      (sortedProviders.length - 1) * PROVIDER_X_GAP
    );

    const PORT_X_GAP = 240;
    const PORT_LAYER_Y = 170;
    const sortedPorts = [...ports].sort((a, b) => a.name.localeCompare(b.name));
    const portRowWidth = Math.max(0, (sortedPorts.length - 1) * PORT_X_GAP);

    const ACTOR_X_GAP = 300;
    const ACTOR_Y_GAP = 210;
    const PORT_TO_ACTOR_GAP = 170;
    const actorOffsetY =
      sortedPorts.length > 0 ? PORT_LAYER_Y + PORT_TO_ACTOR_GAP : 40;

    let actorLayerMaxWidth = 0;
    for (const depth of orderedDepths) {
      const layerActors = orderedActorsByDepth.get(depth) ?? [];
      actorLayerMaxWidth = Math.max(
        actorLayerMaxWidth,
        (layerActors.length - 1) * ACTOR_X_GAP
      );
    }

    const TOOL_X_GAP = 220;
    const sortedToolGroups = Array.from(toolGroups).sort();
    const toolRowWidth = Math.max(
      0,
      (sortedToolGroups.length - 1) * TOOL_X_GAP
    );

    const canvasWidth = Math.max(
      actorLayerMaxWidth,
      toolRowWidth,
      portRowWidth,
      providerRowWidth,
      800
    );

    const providerStartX = (canvasWidth - providerRowWidth) / 2;
    sortedProviders.forEach((p, i) => {
      const isSelected = selectedProviderId === p.provider;
      allNodes.push({
        id: `stage:provider:${p.provider}`,
        position: {
          x: providerStartX + i * PROVIDER_X_GAP,
          y: PROVIDER_LAYER_Y,
        },
        data: { label: p.provider },
        selected: isSelected,
        draggable: false,
        style: {
          borderRadius: "12px",
          border: `1px solid ${isSelected ? "rgba(168, 85, 247, 0.85)" : p.enabled ? "rgba(168, 85, 247, 0.45)" : "rgba(100, 116, 139, 0.35)"}`,
          background: p.enabled
            ? "rgba(250, 245, 255, 0.96)"
            : "rgba(248, 250, 252, 0.96)",
          color: p.enabled ? "rgb(107, 33, 168)" : "rgb(71, 85, 105)",
          fontWeight: 700,
          padding: "8px 12px",
        },
      });
    });

    const portStartX = (canvasWidth - portRowWidth) / 2;
    sortedPorts.forEach((p, i) => {
      const isSelected = selectedPortId === p.id;
      allNodes.push({
        id: `stage:port:${p.id}`,
        position: { x: portStartX + i * PORT_X_GAP, y: PORT_LAYER_Y },
        data: { label: p.name },
        selected: isSelected,
        draggable: false,
        style: {
          borderRadius: "12px",
          border: `1px solid ${isSelected ? "rgba(14, 116, 214, 0.85)" : p.enabled ? "rgba(14, 116, 214, 0.45)" : "rgba(100, 116, 139, 0.35)"}`,
          background: p.enabled
            ? "rgba(239, 246, 255, 0.96)"
            : "rgba(248, 250, 252, 0.96)",
          color: p.enabled ? "rgb(30, 64, 175)" : "rgb(71, 85, 105)",
          fontWeight: 700,
          padding: "8px 12px",
        },
      });
    });

    let maxActorDepth = 0;
    for (const depth of orderedDepths) {
      maxActorDepth = Math.max(maxActorDepth, depth);
      const layerActors = orderedActorsByDepth.get(depth) ?? [];
      const rowWidth = Math.max(0, (layerActors.length - 1) * ACTOR_X_GAP);
      const rowStartX = (canvasWidth - rowWidth) / 2;

      layerActors.forEach((actorId, index) => {
        const actor = actorById.get(actorId);
        if (!actor) return;
        allNodes.push({
          id: actor.id,
          type: "actor",
          position: {
            x: rowStartX + index * ACTOR_X_GAP,
            y: actorOffsetY + depth * ACTOR_Y_GAP,
          },
          data: { actor },
          selected: actor.id === selectedActorId,
        });
      });
    }

    if (sortedToolGroups.length > 0) {
      const startX = (canvasWidth - toolRowWidth) / 2;
      const toolY = actorOffsetY + (maxActorDepth + 1) * ACTOR_Y_GAP + 40;
      sortedToolGroups.forEach((group, i) => {
        allNodes.push({
          id: `stage:tool:${group}`,
          position: { x: startX + i * TOOL_X_GAP, y: toolY },
          data: { label: group },
          draggable: false,
          style: {
            borderRadius: "14px",
            border: "1px solid rgba(217, 119, 6, 0.35)",
            background: "rgba(255, 247, 237, 0.96)",
            color: "rgb(146, 64, 14)",
            fontWeight: 600,
            padding: "10px 14px",
          },
        });
      });
    }

    for (const port of ports) {
      const sourceId = `stage:port:${port.id}`;
      const stroke = colorForProvider(port.provider);

      if (port.assignedActorId && actorIds.has(port.assignedActorId)) {
        allEdges.push({
          id: `stage:edge:port-assigned:${port.id}:${port.assignedActorId}`,
          source: sourceId,
          target: port.assignedActorId,
          style: { stroke, strokeWidth: 2.5 },
          animated: port.enabled,
          label: "assigned",
          labelStyle: { fontSize: "8px", fill: stroke, fontWeight: "bold" },
        });
      }

      for (const actorId of port.actorIds) {
        if (!actorIds.has(actorId) || actorId === port.assignedActorId)
          continue;
        allEdges.push({
          id: `stage:edge:port-bound:${port.id}:${actorId}`,
          source: sourceId,
          target: actorId,
          style: { stroke, strokeWidth: 1.5, strokeDasharray: "5,5" },
          animated: false,
        });
      }
    }

    for (const [left, right] of actorPairs.values()) {
      const leftDepth = actorDepth.get(left) ?? 0;
      const rightDepth = actorDepth.get(right) ?? 0;
      const source = leftDepth <= rightDepth ? left : right;
      const target = source === left ? right : left;
      const sourceActor = actorById.get(source);

      allEdges.push({
        id: `stage:edge:actor:${left}:${right}`,
        source,
        target,
        style: {
          stroke: colorForProvider(sourceActor?.provider ?? ""),
          strokeWidth: 1.6,
        },
        animated: true,
      });
    }

    for (const [actorId, groups] of actorToolGroups.entries()) {
      const actor = actorById.get(actorId);
      const stroke = colorForProvider(actor?.provider ?? "");
      for (const group of groups) {
        allEdges.push({
          id: `stage:edge:tool:${actorId}:${group}`,
          source: actorId,
          target: `stage:tool:${group}`,
          style: { stroke, strokeWidth: 1.3 },
          animated: true,
        });
      }
    }

    for (const actor of actors) {
      if (actor.provider) {
        allEdges.push({
          id: `stage:edge:provider:${actor.provider}:${actor.id}`,
          source: `stage:provider:${actor.provider}`,
          target: actor.id,
          style: { stroke: "rgba(168, 85, 247, 0.2)", strokeWidth: 1 },
          animated: false,
        });
      }
    }

    return { nodes: allNodes, edges: allEdges };
  }, [
    actors,
    ports,
    providers,
    actorGraphMessages,
    selectedActorId,
    selectedPortId,
    selectedProviderId,
  ]);
}
