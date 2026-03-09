import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
  ScrollArea,
} from "@borg/ui";
import {
  ChevronDown,
  History as HistoryIcon,
  Plus,
  Shield,
  Terminal,
  User2,
} from "lucide-react";
import React from "react";
import { ActorContextWindow } from "../types";

type ContextViewProps = {
  window: ActorContextWindow;
};

export const ContextView: React.FC<ContextViewProps> = ({ window }) => {
  return (
    <div className="flex h-full flex-col overflow-hidden bg-white">
      <ScrollArea className="flex-1">
        <div className="divide-y divide-slate-100">
          <Collapsible className="group overflow-hidden">
            <CollapsibleTrigger className="flex w-full items-center justify-between px-4 py-3 text-left hover:bg-slate-50">
              <div className="flex items-center gap-3">
                <Shield className="h-4 w-4 text-slate-400" />
                <span className="text-xs font-bold uppercase tracking-wider text-slate-600">
                  System Prompt
                </span>
              </div>
              <ChevronDown className="h-4 w-4 text-slate-400 transition-transform group-data-[state=open]:rotate-180" />
            </CollapsibleTrigger>
            <CollapsibleContent>
              <div className="bg-slate-50/50 p-4">
                <pre className="whitespace-pre-wrap font-mono text-[11px] leading-relaxed text-slate-700">
                  {window.systemPrompt || "(empty)"}
                </pre>
              </div>
            </CollapsibleContent>
          </Collapsible>

          <Collapsible className="group overflow-hidden">
            <CollapsibleTrigger className="flex w-full items-center justify-between px-4 py-3 text-left hover:bg-slate-50">
              <div className="flex items-center gap-3">
                <User2 className="h-4 w-4 text-slate-400" />
                <span className="text-xs font-bold uppercase tracking-wider text-slate-600">
                  Actor Prompt
                </span>
              </div>
              <ChevronDown className="h-4 w-4 text-slate-400 transition-transform group-data-[state=open]:rotate-180" />
            </CollapsibleTrigger>
            <CollapsibleContent>
              <div className="bg-slate-50/50 p-4">
                <pre className="whitespace-pre-wrap font-mono text-[11px] leading-relaxed text-slate-700">
                  {window.behaviorPrompt || "(empty)"}
                </pre>
              </div>
            </CollapsibleContent>
          </Collapsible>

          <Collapsible className="group overflow-hidden">
            <CollapsibleTrigger className="flex w-full items-center justify-between px-4 py-3 text-left hover:bg-slate-50">
              <div className="flex items-center gap-3">
                <Terminal className="h-4 w-4 text-amber-500" />
                <span className="text-xs font-bold uppercase tracking-wider text-slate-600">
                  Available Tools ({window.availableTools.length})
                </span>
              </div>
              <ChevronDown className="h-4 w-4 text-slate-400 transition-transform group-data-[state=open]:rotate-180" />
            </CollapsibleTrigger>
            <CollapsibleContent>
              <div className="divide-y divide-slate-50 bg-slate-50/30">
                {window.availableTools.map((tool) => (
                  <Collapsible
                    key={tool.name}
                    className="group/tool overflow-hidden"
                  >
                    <CollapsibleTrigger className="flex w-full items-center justify-between pl-11 pr-4 py-2 text-left hover:bg-amber-50/50">
                      <span className="font-mono text-[11px] font-semibold text-amber-700">
                        {tool.name}
                      </span>
                      <Plus className="h-3.5 w-3.5 text-slate-300 transition-transform group-data-[state=open]/tool:rotate-45" />
                    </CollapsibleTrigger>
                    <CollapsibleContent className="bg-white px-11 py-3">
                      <p className="mb-3 text-[11px] leading-relaxed text-slate-600">
                        {tool.description}
                      </p>
                      <div className="rounded-lg border border-slate-100 bg-slate-50 p-3">
                        <pre className="font-mono text-[10px] text-slate-500">
                          {JSON.stringify(tool.parameters, null, 2)}
                        </pre>
                      </div>
                    </CollapsibleContent>
                  </Collapsible>
                ))}
              </div>
            </CollapsibleContent>
          </Collapsible>

          <div className="p-4">
            <div className="mb-4 flex items-center gap-3">
              <HistoryIcon className="h-4 w-4 text-slate-400" />
              <span className="text-xs font-bold uppercase tracking-wider text-slate-600">
                Message History ({window.orderedMessages.length} turns)
              </span>
            </div>
            <div className="space-y-3">
              {window.orderedMessages.map((msg, i) => (
                <div
                  key={i}
                  className={`rounded-2xl border p-3 shadow-sm ${
                    msg.type === "user"
                      ? "border-sky-100 bg-sky-50/20"
                      : msg.type === "assistant"
                        ? "border-slate-200 bg-white"
                        : "border-slate-100 bg-slate-50/50"
                  }`}
                >
                  <div className="mb-1.5 flex items-center justify-between gap-2 text-[10px] font-bold uppercase tracking-tight text-slate-400">
                    <span>{msg.role || msg.type}</span>
                  </div>
                  <p className="whitespace-pre-wrap text-[12px] leading-relaxed text-slate-700">
                    {msg.content}
                  </p>
                  {msg.toolCalls?.map((call: any) => (
                    <div
                      key={call.id}
                      className="mt-3 rounded-xl border border-amber-100 bg-amber-50/30 p-3 font-mono text-[10px]"
                    >
                      <div className="mb-2 flex items-center gap-2">
                        <Terminal className="h-3 w-3 text-amber-600" />
                        <span className="font-bold text-amber-700">
                          CALL: {call.name}
                        </span>
                      </div>
                      <pre className="text-slate-600">
                        {JSON.stringify(call.arguments, null, 2)}
                      </pre>
                    </div>
                  ))}
                </div>
              ))}
            </div>
          </div>
        </div>
      </ScrollArea>
    </div>
  );
};
