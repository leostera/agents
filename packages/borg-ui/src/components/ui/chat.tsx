"use client";

import { ArrowUp, Square } from "lucide-react";
import * as React from "react";

import { cn } from "@/lib/utils";

export type ChatRole = "assistant" | "user" | "system";

export type ChatMessageItem = {
  id: string;
  role: ChatRole;
  text: string;
  timestamp?: string | null;
};

type ChatThreadProps = {
  messages: ChatMessageItem[];
  isLoading?: boolean;
  emptyTitle?: string;
  emptyDescription?: string;
  className?: string;
  children?: React.ReactNode;
};

export function ChatThread({
  messages,
  isLoading = false,
  emptyTitle = "No messages yet",
  emptyDescription = "Session messages will appear here.",
  className,
  children,
}: ChatThreadProps) {
  return (
    <section className={cn("flex h-full min-h-0 flex-col", className)}>
      <div className="min-h-0 flex-1 overflow-y-auto px-3 py-3 md:px-4 md:py-4">
        {messages.length === 0 ? (
          <div className="flex h-full min-h-[180px] flex-col items-center justify-center text-center">
            <p className="text-sm font-semibold">{emptyTitle}</p>
            <p className="text-muted-foreground mt-1 text-xs">{emptyDescription}</p>
          </div>
        ) : (
          <div className="mx-auto flex w-full max-w-3xl flex-col gap-3">
            {messages.map((message) => (
              <ChatMessage key={message.id} message={message} />
            ))}
            {isLoading ? <ChatLoadingIndicator /> : null}
          </div>
        )}
      </div>
      {children ? <div className="mx-auto w-full max-w-3xl">{children}</div> : null}
    </section>
  );
}

type ChatMessageProps = {
  message: ChatMessageItem;
};

export function ChatMessage({ message }: ChatMessageProps) {
  const isUser = message.role === "user";
  const toneClass =
    message.role === "assistant"
      ? "bg-muted text-foreground"
      : message.role === "system"
        ? "border border-dashed border-border bg-background text-muted-foreground"
        : "bg-primary text-primary-foreground";

  return (
    <article className={cn("flex", isUser ? "justify-end" : "justify-start")}>
      <div
        className={cn(
          "max-w-[82%] whitespace-pre-wrap break-words rounded-2xl px-4 py-2.5 text-sm",
          toneClass,
        )}
      >
        <p className="mb-1 text-[10px] uppercase tracking-[0.1em] opacity-70">
          {message.role}
        </p>
        <p>{message.text}</p>
        {message.timestamp ? (
          <p className="mt-1 text-[10px] opacity-70">{message.timestamp}</p>
        ) : null}
      </div>
    </article>
  );
}

export function ChatLoadingIndicator() {
  return (
    <div className="flex items-center gap-1.5 py-2">
      <span className="size-1.5 animate-bounce rounded-full bg-muted-foreground/60 [animation-delay:-0.3s]" />
      <span className="size-1.5 animate-bounce rounded-full bg-muted-foreground/60 [animation-delay:-0.15s]" />
      <span className="size-1.5 animate-bounce rounded-full bg-muted-foreground/60" />
    </div>
  );
}

type ChatComposerShellProps = {
  value: string;
  onChange: (value: string) => void;
  onSubmit: () => void;
  isRunning?: boolean;
  onCancel?: () => void;
  placeholder?: string;
  className?: string;
};

export function ChatComposerShell({
  value,
  onChange,
  onSubmit,
  isRunning = false,
  onCancel,
  placeholder = "Send a message...",
  className,
}: ChatComposerShellProps) {
  const handleSubmit = (event: React.FormEvent) => {
    event.preventDefault();
    if (!isRunning && value.trim()) {
      onSubmit();
    }
  };

  return (
    <form onSubmit={handleSubmit} className={cn("p-3 pt-2", className)}>
      <div className="flex items-end gap-2 rounded-2xl border bg-background px-3 py-2 shadow-sm">
        <textarea
          value={value}
          onChange={(event) => onChange(event.currentTarget.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && !event.shiftKey) {
              event.preventDefault();
              if (!isRunning && value.trim()) {
                onSubmit();
              }
            }
          }}
          placeholder={placeholder}
          className="max-h-32 min-h-8 flex-1 resize-none bg-transparent py-1 text-sm leading-normal outline-none placeholder:text-muted-foreground"
          rows={1}
        />
        {isRunning ? (
          <button
            type="button"
            onClick={onCancel}
            className="flex size-8 shrink-0 items-center justify-center rounded-full bg-destructive text-destructive-foreground transition-colors hover:bg-destructive/90"
            aria-label="Stop generating"
          >
            <Square className="size-3 fill-current" />
          </button>
        ) : (
          <button
            type="submit"
            disabled={!value.trim()}
            className={cn(
              "flex size-8 shrink-0 items-center justify-center rounded-full transition-colors",
              value.trim()
                ? "bg-primary text-primary-foreground hover:bg-primary/90"
                : "bg-muted text-muted-foreground",
            )}
            aria-label="Send message"
          >
            <ArrowUp className="size-4" />
          </button>
        )}
      </div>
    </form>
  );
}
