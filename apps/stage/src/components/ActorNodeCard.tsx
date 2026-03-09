import React from "react";
import { Handle, type NodeProps, Position } from "reactflow";
import { ActorSummary } from "../types";

export type ActorNodeData = {
  actor: ActorSummary;
  onToggleStatus: (actorId: string, newStatus: string) => void;
};

export const ActorNodeCard: React.FC<NodeProps<ActorNodeData>> = ({
  data,
  selected,
}) => {
  const actor = data.actor;
  const onToggleStatus = data.onToggleStatus;
  const isRunning = actor.status.toUpperCase() === "RUNNING";
  const hasRuntimeConfig =
    actor.provider.trim().length > 0 && actor.model.trim().length > 0;

  return (
    <div
      className={`stage-card w-48 rounded-2xl border px-3 py-2 shadow-md transition ${
        selected
          ? "border-sky-400 bg-sky-50"
          : "border-slate-200 bg-white/90 hover:border-slate-300"
      }`}
    >
      <div className="flex items-center justify-between gap-2">
        <div className="flex min-w-0 items-center gap-2">
          <span
            className={`h-2.5 w-2.5 shrink-0 rounded-full ${
              isRunning ? "bg-emerald-500" : "bg-slate-400"
            }`}
          />
          <p className="truncate text-xs font-semibold">{actor.name}</p>
        </div>
        <div className="flex items-center gap-1">
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              onToggleStatus(actor.id, isRunning ? "PAUSED" : "RUNNING");
            }}
            className={`rounded px-1.5 py-0.5 text-[10px] font-medium ${
              isRunning
                ? "bg-amber-100 text-amber-700 hover:bg-amber-200"
                : "bg-emerald-100 text-emerald-700 hover:bg-emerald-200"
            }`}
          >
            {isRunning ? "⏸" : "▶"}
          </button>
        </div>
      </div>
      <p className="mt-1 truncate text-[11px] text-slate-600">
        {hasRuntimeConfig
          ? `${actor.provider} / ${actor.model}`
          : "provider/model not set"}
      </p>
      <Handle
        type="target"
        position={Position.Top}
        className="!h-2 !w-2 !opacity-0"
      />
      <Handle
        type="source"
        position={Position.Bottom}
        className="!h-2 !w-2 !opacity-0"
      />
    </div>
  );
};
