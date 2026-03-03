"use client";

import { ArrowUp, Square } from "lucide-react";
import * as React from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

import { cn } from "@/lib/utils";

export type ChatRole = "assistant" | "user" | "system";

export type ChatMessageItem = {
  id: string;
  role: ChatRole;
  text: string;
  timestamp?: string | null;
  pending?: boolean;
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
  const endRef = React.useRef<HTMLDivElement | null>(null);

  React.useEffect(() => {
    endRef.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  }, [messages, isLoading]);

  return (
    <section
      className={cn("flex h-full min-h-0 flex-col bg-background", className)}
    >
      <div className="relative min-h-0 flex-1 overflow-x-auto overflow-y-auto px-4 pt-4">
        {messages.length === 0 ? (
          <div className="mx-auto flex h-full min-h-[260px] w-full max-w-3xl flex-col items-start justify-center px-2">
            <p className="font-semibold text-2xl">{emptyTitle}</p>
            <p className="text-muted-foreground mt-2 text-lg">
              {emptyDescription}
            </p>
          </div>
        ) : (
          <div className="mx-auto flex w-full max-w-3xl flex-col gap-2 pb-4">
            {messages.map((message) => (
              <ChatMessage key={message.id} message={message} />
            ))}
            {isLoading ? <ChatLoadingIndicator /> : null}
            <div ref={endRef} />
          </div>
        )}
      </div>
      {children ? (
        <div className="sticky bottom-0 mx-auto w-full max-w-3xl bg-background pb-4">
          {children}
        </div>
      ) : null}
    </section>
  );
}

type ChatMessageProps = {
  message: ChatMessageItem;
};

export function ChatMessage({ message }: ChatMessageProps) {
  const isUser = message.role === "user";
  const timestampLabel =
    typeof message.timestamp === "string" && message.timestamp.trim().length > 0
      ? message.timestamp
      : "just now";
  const toneClass =
    message.role === "assistant"
      ? "text-foreground"
      : message.role === "system"
        ? "border border-dashed border-border bg-background text-muted-foreground"
        : "bg-muted text-foreground";

  return (
    <article className={cn("flex", isUser ? "justify-end" : "justify-start")}>
      <div
        className={cn(
          "max-w-[85%] whitespace-pre-wrap break-words rounded-2xl px-4 py-2.5 text-sm",
          toneClass,
          message.role === "assistant" ? "px-2 py-1" : "",
          message.pending ? "opacity-70" : ""
        )}
      >
        <div className="chat-markdown markdown-body text-sm">
          <ReactMarkdown remarkPlugins={[remarkGfm]}>
            {message.text}
          </ReactMarkdown>
        </div>
        <div
          className={cn(
            "mt-1 flex items-center gap-2 text-[10px] opacity-70",
            isUser ? "justify-end" : "justify-start"
          )}
        >
          <p>{timestampLabel}</p>
          {message.pending ? <span>sending...</span> : null}
        </div>
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
  const formRef = React.useRef<HTMLFormElement | null>(null);

  const handleSubmit = (event: React.FormEvent) => {
    event.preventDefault();
    if (!isRunning && value.trim()) {
      onSubmit();
    }
  };

  return (
    <form
      ref={formRef}
      onSubmit={handleSubmit}
      className={cn("p-3 pt-2", className)}
    >
      <div className="flex items-end gap-2 rounded-2xl border border-input bg-background px-3 py-2">
        <textarea
          value={value}
          onChange={(event) => onChange(event.currentTarget.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && !event.shiftKey) {
              event.preventDefault();
              formRef.current?.requestSubmit();
            }
          }}
          placeholder={placeholder}
          className="max-h-32 min-h-12 flex-1 resize-none bg-transparent py-1 text-sm leading-normal outline-none placeholder:text-muted-foreground"
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
                : "bg-muted text-muted-foreground"
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
