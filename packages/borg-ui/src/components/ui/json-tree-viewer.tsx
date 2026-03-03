import { ChevronDown, ChevronRight } from "lucide-react";
import * as React from "react";

import { cn } from "../../lib/utils";

type JsonTreeViewerProps = {
  value: unknown;
  className?: string;
  defaultExpandedDepth?: number;
};

type JsonValueKind =
  | "null"
  | "string"
  | "number"
  | "boolean"
  | "array"
  | "object";

function valueKind(value: unknown): JsonValueKind {
  if (value === null) return "null";
  if (Array.isArray(value)) return "array";
  switch (typeof value) {
    case "string":
      return "string";
    case "number":
      return "number";
    case "boolean":
      return "boolean";
    case "object":
      return "object";
    default:
      return "string";
  }
}

function primitiveLabel(value: unknown): string {
  const kind = valueKind(value);
  if (kind === "null") return "null";
  if (kind === "string") return JSON.stringify(value);
  return String(value);
}

function previewLabel(value: unknown): string {
  const kind = valueKind(value);
  if (kind === "array") {
    const size = Array.isArray(value) ? value.length : 0;
    return `Array(${size})`;
  }
  if (kind === "object") {
    const size =
      value && typeof value === "object"
        ? Object.keys(value as Record<string, unknown>).length
        : 0;
    return `Object(${size})`;
  }
  return primitiveLabel(value);
}

function valueClass(value: unknown): string {
  switch (valueKind(value)) {
    case "string":
      return "text-emerald-600";
    case "number":
      return "text-blue-600";
    case "boolean":
      return "text-violet-600";
    case "null":
      return "text-muted-foreground";
    default:
      return "text-foreground";
  }
}

function entriesOf(value: unknown): Array<[string, unknown]> {
  if (Array.isArray(value)) {
    return value.map((entry, index) => [String(index), entry]);
  }
  if (value && typeof value === "object") {
    return Object.entries(value as Record<string, unknown>).sort(([a], [b]) =>
      a.localeCompare(b)
    );
  }
  return [];
}

type JsonNodeProps = {
  label?: string;
  value: unknown;
  depth: number;
  defaultExpandedDepth: number;
};

function JsonNode({
  label,
  value,
  depth,
  defaultExpandedDepth,
}: JsonNodeProps) {
  const kind = valueKind(value);
  const isBranch = kind === "array" || kind === "object";
  const [isOpen, setIsOpen] = React.useState(depth < defaultExpandedDepth);
  const entries = React.useMemo(() => entriesOf(value), [value]);

  if (!isBranch) {
    return (
      <div className="flex items-start gap-2">
        <span className="w-4 shrink-0" />
        {label ? (
          <span className="text-foreground/80 font-mono text-xs">{label}:</span>
        ) : null}
        <span className={cn("font-mono text-xs", valueClass(value))}>
          {primitiveLabel(value)}
        </span>
      </div>
    );
  }

  return (
    <div className="space-y-1">
      <button
        type="button"
        onClick={() => setIsOpen((prev) => !prev)}
        className="hover:bg-muted/60 flex w-full items-center gap-2 rounded px-1 py-0.5 text-left"
      >
        {isOpen ? (
          <ChevronDown className="text-muted-foreground size-3.5 shrink-0" />
        ) : (
          <ChevronRight className="text-muted-foreground size-3.5 shrink-0" />
        )}
        {label ? (
          <span className="text-foreground/80 font-mono text-xs">{label}:</span>
        ) : null}
        <span className="text-muted-foreground font-mono text-xs">
          {previewLabel(value)}
        </span>
      </button>

      {isOpen ? (
        <div className="border-border/70 ml-2 space-y-1 border-l pl-2">
          {entries.length === 0 ? (
            <div className="text-muted-foreground font-mono text-xs">empty</div>
          ) : (
            entries.map(([key, child]) => (
              <JsonNode
                key={key}
                label={key}
                value={child}
                depth={depth + 1}
                defaultExpandedDepth={defaultExpandedDepth}
              />
            ))
          )}
        </div>
      ) : null}
    </div>
  );
}

export function JsonTreeViewer({
  value,
  className,
  defaultExpandedDepth = 2,
}: JsonTreeViewerProps) {
  return (
    <div className={cn("font-mono text-xs", className)}>
      <JsonNode
        value={value}
        depth={0}
        defaultExpandedDepth={defaultExpandedDepth}
      />
    </div>
  );
}
