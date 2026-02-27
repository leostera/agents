import * as React from "react"

import { cn } from "../../lib/utils"
import { Link } from "./link"

type EntityViewProps = React.ComponentProps<"section"> & {
  uri: string
  kind?: string
  fields?: Record<string, unknown>
}

const URI_RE = /^[a-z][a-z0-9+.-]*:[^:\s]+:[^:\s]+$/i
const TX_URI_RE = /^borg:tx:/i

function formatEntityValue(value: unknown): string {
  if (value === null) return "null"
  if (value === undefined) return "—"
  if (typeof value === "string") return value
  if (typeof value === "number" || typeof value === "boolean") return String(value)
  try {
    return JSON.stringify(value)
  } catch {
    return String(value)
  }
}

function entityHref(uri: string): string {
  return `/memory/entity/${uri}`
}

function isEntityLinkableUri(uri: string): boolean {
  return URI_RE.test(uri) && !TX_URI_RE.test(uri)
}

function formatKind(kind?: string): string {
  const trimmed = kind?.trim()
  if (!trimmed) return "unknown"
  if (trimmed.includes(":")) return trimmed
  return `borg:kind:${trimmed}`
}

function renderEntityValue(value: unknown): React.ReactNode {
  if (typeof value === "string") {
    if (isEntityLinkableUri(value)) {
      return (
        <Link href={entityHref(value)} className="text-primary underline underline-offset-2">
          {value}
        </Link>
      )
    }
    return value
  }

  if (Array.isArray(value)) {
    return (
      <div className="space-y-1">
        {value.map((item, index) => (
          <div key={index} className="border-border/50 border-l pl-2">
            {renderEntityValue(item)}
          </div>
        ))}
      </div>
    )
  }

  if (value && typeof value === "object") {
    return (
      <div className="space-y-2">
        {Object.entries(value as Record<string, unknown>)
          .sort(([a], [b]) => a.localeCompare(b))
          .map(([nestedKey, nestedValue]) => (
            <div key={nestedKey}>
              <p className="text-muted-foreground text-xs">{nestedKey}</p>
              <div className="break-words">{renderEntityValue(nestedValue)}</div>
            </div>
          ))}
      </div>
    )
  }

  return formatEntityValue(value)
}

export function EntityView({ uri, kind, fields, className, ...props }: EntityViewProps) {
  const orderedFields = React.useMemo(
    () =>
      Object.entries(fields ?? {})
        .filter(([key]) => !["uri", "kind", "namespace"].includes(key.toLowerCase()))
        .sort(([a], [b]) => a.localeCompare(b)),
    [fields]
  )

  return (
    <section className={cn("space-y-3 text-sm", className)} {...props}>
      <div>
        <p className="text-muted-foreground text-xs">URI</p>
        <p className="font-mono text-xs break-all">
          {isEntityLinkableUri(uri) ? (
            <Link href={entityHref(uri)} className="text-primary underline underline-offset-2">
              {uri}
            </Link>
          ) : (
            uri
          )}
        </p>
      </div>
      <div>
        <p className="text-muted-foreground text-xs">Kind</p>
        <p>{formatKind(kind)}</p>
      </div>
      {orderedFields.map(([field, value]) => (
        <div key={field}>
          <p className="text-muted-foreground text-xs">{field}</p>
          <div className="break-words">{renderEntityValue(value)}</div>
        </div>
      ))}
    </section>
  )
}
