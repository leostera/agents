import { JsonTreeViewer } from "@borg/ui";
import { History, MessageSquare, Shield, Terminal, User2 } from "lucide-react";
import React from "react";
import { MailboxMessage } from "../types";

type MessageBubbleProps = {
  message: MailboxMessage;
  actorName?: string;
  formatDate: (value: string) => string;
};

export const MessageBubble: React.FC<MessageBubbleProps> = ({
  message,
  actorName,
  formatDate,
}) => {
  const normalizedRole = (message.role ?? "").toLowerCase();
  const isUser = normalizedRole === "user";
  const isAssistant = normalizedRole === "assistant";
  const isSystem = normalizedRole === "system";

  const roleLabel = isAssistant
    ? (actorName ?? "assistant")
    : (message.role ?? "system");

  const parsedPayload = parseJsonPayload(message.payload);

  // Try to find a nested payload if this is a wrapper (like user_text or assistant_text)
  let effectivePayload = getEffectivePayload(parsedPayload);

  // If we have a text field that is itself JSON, try to parse it
  if (typeof message.text === "string" && message.text.trim().startsWith("{")) {
    const parsedText = parseJsonPayload(message.text);
    if (
      parsedText &&
      typeof parsedText === "object" &&
      (parsedText.type || parsedText.kind)
    ) {
      effectivePayload = parsedText;
    }
  }

  const hasText = (message.text ?? "").trim().length > 0;

  // Specific rendering for port messages
  if (isPortMessage(effectivePayload)) {
    return (
      <article
        className={`max-w-[94%] rounded-2xl border px-3 py-2 text-xs shadow-sm ml-auto border-sky-300 bg-sky-50`}
      >
        <div className="mb-1 flex items-center justify-between gap-2 text-[10px] text-sky-600 uppercase font-bold">
          <div className="flex items-center gap-1">
            <User2 className="h-3 w-3" />
            <span>{effectivePayload.user_id}</span>
          </div>
          <span>{formatDate(message.createdAt)}</span>
        </div>
        <p className="whitespace-pre-wrap text-[12px] text-slate-700">
          {effectivePayload.text}
        </p>
        <div className="mt-1 flex items-center justify-between gap-2 text-[9px] text-sky-400 font-mono">
          <span>{message.messageType}</span>
          <span>Port: {effectivePayload.port_context?.port ?? "unknown"}</span>
        </div>
      </article>
    );
  }

  // Specific rendering for structured actor messages
  if (isActorMessage(effectivePayload)) {
    return (
      <article
        className={`max-w-[94%] rounded-2xl border px-3 py-2 text-xs shadow-sm mr-auto border-purple-300 bg-purple-50`}
      >
        <div className="mb-1 flex items-center justify-between gap-2 text-[10px] text-purple-500 uppercase font-bold">
          <div className="flex items-center gap-1">
            <MessageSquare className="h-3 w-3" />
            <span>{effectivePayload.sender_actor_id}</span>
          </div>
          <span>{formatDate(message.createdAt)}</span>
        </div>
        <p className="whitespace-pre-wrap text-[12px] text-slate-700">
          {effectivePayload.text}
        </p>
        <div className="mt-1 flex items-center justify-between gap-2 text-[9px] text-purple-400 font-mono">
          <span>{message.messageType}</span>
          {effectivePayload.submission_id && (
            <span>ID: {effectivePayload.submission_id}</span>
          )}
        </div>
      </article>
    );
  }

  // Specific rendering for structured actor messages
  if (!hasText && isActorMessage(parsedPayload)) {
    return (
      <article
        className={`max-w-[94%] rounded-2xl border px-3 py-2 text-xs shadow-sm mr-auto border-purple-300 bg-purple-50`}
      >
        <div className="mb-1 flex items-center justify-between gap-2 text-[10px] text-purple-500 uppercase font-bold">
          <div className="flex items-center gap-1">
            <MessageSquare className="h-3 w-3" />
            <span>{parsedPayload.sender_actor_id}</span>
          </div>
          <span>{formatDate(message.createdAt)}</span>
        </div>
        <p className="whitespace-pre-wrap text-[12px] text-slate-700">
          {parsedPayload.text}
        </p>
        <div className="mt-1 flex items-center justify-between gap-2 text-[9px] text-purple-400 font-mono">
          <span>{message.messageType}</span>
          {parsedPayload.submission_id && (
            <span>ID: {parsedPayload.submission_id}</span>
          )}
        </div>
      </article>
    );
  }

  return (
    <article
      className={`max-w-[94%] rounded-2xl border px-3 py-2 text-xs shadow-sm ${
        isUser
          ? "ml-auto border-sky-300 bg-sky-50"
          : isAssistant
            ? "mr-auto border-emerald-300 bg-emerald-50"
            : "mr-auto border-slate-300 bg-slate-50"
      }`}
    >
      <div className="mb-1 flex items-center justify-between gap-2 text-[10px] text-slate-500 uppercase font-bold">
        <div className="flex items-center gap-1">
          {isUser ? (
            <User2 className="h-3 w-3" />
          ) : isSystem ? (
            <Shield className="h-3 w-3" />
          ) : null}
          <span>{roleLabel}</span>
        </div>
        <span>{formatDate(message.createdAt)}</span>
      </div>

      {hasText ? (
        <p className="whitespace-pre-wrap text-[12px] text-slate-700">
          {message.text}
        </p>
      ) : parsedPayload !== null && parsedPayload !== undefined ? (
        typeof parsedPayload === "object" ? (
          <div className="rounded-lg border border-slate-200 bg-white p-2">
            <JsonTreeViewer value={parsedPayload} defaultExpandedDepth={1} />
          </div>
        ) : (
          <p className="whitespace-pre-wrap text-[12px] text-slate-700">
            {String(parsedPayload)}
          </p>
        )
      ) : (
        <p className="text-[12px] text-slate-500">(empty payload)</p>
      )}

      <p className="mt-1 text-[10px] text-slate-500 font-mono">
        {message.messageType}
      </p>
    </article>
  );
};

function parseJsonPayload(payload: unknown): any {
  if (typeof payload !== "string") {
    return payload;
  }
  const trimmed = payload.trim();
  if (!trimmed) {
    return payload;
  }
  try {
    return JSON.parse(trimmed);
  } catch {
    return payload;
  }
}

function getEffectivePayload(payload: any): any {
  if (!payload || typeof payload !== "object") {
    return payload;
  }

  // Handle common wrappers
  const body = payload.body ?? payload.content ?? payload.data;
  if (body) {
    const text = body.text ?? body.content;
    if (typeof text === "string") {
      const parsed = parseJsonPayload(text);
      if (parsed && typeof parsed === "object") {
        return parsed;
      }
    }
    return body;
  }

  return payload;
}

function isActorMessage(payload: any): payload is {
  type: "actor_message";
  sender_actor_id: string;
  text: string;
  submission_id?: string;
} {
  return (
    payload &&
    typeof payload === "object" &&
    payload.type === "actor_message" &&
    typeof payload.sender_actor_id === "string" &&
    typeof payload.text === "string"
  );
}

function isPortMessage(payload: any): payload is {
  kind: "port_message";
  user_id: string;
  text: string;
  port_context: { port: string };
} {
  return (
    payload &&
    typeof payload === "object" &&
    payload.kind === "port_message" &&
    typeof payload.user_id === "string" &&
    typeof payload.text === "string" &&
    typeof payload.port_context === "object" &&
    payload.port_context !== null
  );
}
