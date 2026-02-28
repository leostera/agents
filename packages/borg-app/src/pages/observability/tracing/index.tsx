import React from "react";
import { TracingSection } from "../../../sections/observability/TracingSection";
import { ObservabilityLlmCallsPage } from "./llm-calls";

function normalizePathname(pathname: string): string {
  return pathname.replace(/\/+$/, "") || "/";
}

function resolveTracingSubmenu(pathname: string): "traces" | "llm-calls" {
  const normalizedPathname = normalizePathname(pathname);
  if (
    normalizedPathname === "/observability/tracing/llm-calls" ||
    normalizedPathname.startsWith("/observability/tracing/llm-calls/")
  ) {
    return "llm-calls";
  }
  return "traces";
}

export function ObservabilityTracingPage() {
  const submenu = resolveTracingSubmenu(window.location.pathname);

  return (
    <section>
      {submenu === "llm-calls" ? (
        <ObservabilityLlmCallsPage />
      ) : (
        <TracingSection />
      )}
    </section>
  );
}
