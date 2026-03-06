import React from "react";
import { matchPath, Navigate, type RouteObject, useParams } from "react-router-dom";
import { ObservabilityAlertsPage } from "../pages/observability/alerts";
import { ObservabilityOverviewPage } from "../pages/observability";
import { ObservabilityTracingPage } from "../pages/observability/tracing";
import { OverviewPage } from "../pages/overview";
import { ProviderDetailsPage } from "../pages/settings/providers/id";
import { ProvidersPage } from "../pages/settings/providers";
import { DEFAULT_SECTION_ID } from "./navigation";

function decodeRouteParam(value: string | undefined): string {
  if (!value) return "";
  try {
    return decodeURIComponent(value);
  } catch {
    return value;
  }
}

function ProviderDetailsRoute() {
  const { providerId } = useParams<{ providerId: string }>();
  return <ProviderDetailsPage providerId={decodeRouteParam(providerId)} />;
}

export const dashboardControlRoutes: RouteObject[] = [
  { index: true, element: <OverviewPage /> },
  { path: "overview", element: <Navigate to="/" replace /> },
  { path: "dashboard", element: <Navigate to="/" replace /> },
  { path: "dashbaord", element: <Navigate to="/" replace /> },

  { path: "settings", element: <Navigate to="/settings/providers" replace /> },
  { path: "settings/provider", element: <Navigate to="/settings/providers" replace /> },
  { path: "settings/providers", element: <ProvidersPage /> },
  { path: "settings/provider/:providerId", element: <ProviderDetailsRoute /> },
  { path: "settings/providers/:providerId", element: <ProviderDetailsRoute /> },

  { path: "observability", element: <ObservabilityOverviewPage /> },
  { path: "observability/overview", element: <Navigate to="/observability" replace /> },
  { path: "observability/alerts", element: <ObservabilityAlertsPage /> },
  { path: "observability/traces", element: <Navigate to="/observability/tracing" replace /> },
  { path: "observability/tracing", element: <ObservabilityTracingPage /> },
  { path: "observability/tracing/llm-calls", element: <Navigate to="/observability/tracing" replace /> },
  { path: "observability/tracing/llm-calls/:callId", element: <Navigate to="/observability/tracing" replace /> },
  { path: "observability/tracing/tool-calls", element: <Navigate to="/observability/tracing" replace /> },
  { path: "observability/tracing/tool-calls/:callId", element: <Navigate to="/observability/tracing" replace /> },

  { path: "control/*", element: <Navigate to="/" replace /> },
  { path: "memory/*", element: <Navigate to="/" replace /> },
  { path: "taskgraph/*", element: <Navigate to="/" replace /> },
  { path: "fs/*", element: <Navigate to="/" replace /> },

  { path: "*", element: <Navigate to="/" replace /> },
];

type SectionMatcher = {
  id: string;
  path: string;
  end?: boolean;
};

const SECTION_MATCHERS: SectionMatcher[] = [
  { id: "settings-providers", path: "/settings/providers/:providerId" },
  { id: "settings-providers", path: "/settings/provider/:providerId" },
  { id: "settings-providers", path: "/settings/providers", end: true },
  { id: "observability-tracing", path: "/observability/tracing", end: true },
  { id: "observability-alerts", path: "/observability/alerts", end: true },
  { id: "observability-overview", path: "/observability", end: true },
  { id: "overview-home", path: "/", end: true },
  { id: "overview-home", path: "/overview", end: true },
  { id: "overview-home", path: "/dashboard", end: true },
  { id: "overview-home", path: "/dashbaord", end: true },
];

export function resolveActiveSectionId(pathname: string): string {
  for (const matcher of SECTION_MATCHERS) {
    if (matchPath({ path: matcher.path, end: matcher.end ?? false }, pathname)) {
      return matcher.id;
    }
  }

  if (matchPath({ path: "/settings/*", end: false }, pathname)) {
    return "settings-providers";
  }
  if (matchPath({ path: "/observability/*", end: false }, pathname)) {
    return "observability-overview";
  }

  return DEFAULT_SECTION_ID;
}
