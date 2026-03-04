import React from "react";
import { TracingSection } from "../../../sections/observability/TracingSection";
import { ObservabilityLlmCallsPage } from "./llm-calls";
import { ObservabilityToolCallsPage } from "./tool-calls";

function normalizePathname(pathname: string): string {
  return pathname.replace(/\/+$/, "") || "/";
}

function resolveTracingSubmenu(
  pathname: string
): "traces" | "llm-calls" | "tool-calls" {
  const normalizedPathname = normalizePathname(pathname);
  if (
    normalizedPathname === "/observability/tracing/llm-calls" ||
    normalizedPathname.startsWith("/observability/tracing/llm-calls/")
  ) {
    return "llm-calls";
  }
  if (
    normalizedPathname === "/observability/tracing/tool-calls" ||
    normalizedPathname.startsWith("/observability/tracing/tool-calls/")
  ) {
    return "tool-calls";
  }
  return "traces";
}

export function ObservabilityTracingPage() {
  const submenu = resolveTracingSubmenu(window.location.pathname);

  return (
    <section>
      {submenu === "llm-calls" ? (
        <ObservabilityLlmCallsPage />
      ) : submenu === "tool-calls" ? (
        <ObservabilityToolCallsPage />
      ) : (
        <TracingSection />
      )}
    </section>
  );
}
