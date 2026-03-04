import React from "react";
import {
  matchPath,
  Navigate,
  type RouteObject,
  useParams,
} from "react-router-dom";
import { ActorsPage } from "../pages/control/actors";
import { ActorDetailsPage } from "../pages/control/actors/id";
import { AppsPage } from "../pages/control/apps";
import { AppDetailsPage } from "../pages/control/apps/id";
import { BehaviorsPage } from "../pages/control/behaviors";
import { BehaviorDetailsPage } from "../pages/control/behaviors/id";
import { ClockworkPage } from "../pages/control/clockwork";
import { PortsPage } from "../pages/control/ports";
import { PortDetailsPage } from "../pages/control/ports/id";
import { SessionPage } from "../pages/control/sessions";
import { SessionDetailsPage } from "../pages/control/sessions/id";
import { UsersPage } from "../pages/control/users";
import { UserDetailsPage } from "../pages/control/users/id";
import { FsExplorerPage } from "../pages/fs/explorer";
import { FsSettingsPage } from "../pages/fs/settings";
import { MemoryEntityPage } from "../pages/memory/entity";
import { MemoryExplorerPage } from "../pages/memory/explorer";
import { MemoryGraphPage } from "../pages/memory/graph";
import { ObservabilityOverviewPage } from "../pages/observability";
import { ObservabilityAlertsPage } from "../pages/observability/alerts";
import { ObservabilityTracingPage } from "../pages/observability/tracing";
import { ObservabilityLlmCallsPage } from "../pages/observability/tracing/llm-calls";
import { ObservabilityToolCallsPage } from "../pages/observability/tracing/tool-calls";
import { OverviewPage } from "../pages/overview";
import { ProvidersPage } from "../pages/settings/providers";
import { ProviderDetailsPage } from "../pages/settings/providers/id";
import { TaskGraphExplorerPage } from "../pages/taskgraph/explorer";
import { TaskGraphKanbanPage } from "../pages/taskgraph/kanban";
import { TaskGraphTaskDetailsPage } from "../pages/taskgraph/task";
import { DEFAULT_SECTION_ID } from "./navigation";

function decodeRouteParam(value: string | undefined): string {
  if (!value) return "";
  try {
    return decodeURIComponent(value);
  } catch {
    return value;
  }
}

function SessionDetailsRoute() {
  const { sessionId } = useParams<{ sessionId: string }>();
  return <SessionDetailsPage sessionId={decodeRouteParam(sessionId)} />;
}

function BehaviorDetailsRoute() {
  const { behaviorId } = useParams<{ behaviorId: string }>();
  return <BehaviorDetailsPage behaviorId={decodeRouteParam(behaviorId)} />;
}

function ActorDetailsRoute() {
  const { actorId } = useParams<{ actorId: string }>();
  return <ActorDetailsPage actorId={decodeRouteParam(actorId)} />;
}

function AppDetailsRoute() {
  const { appId } = useParams<{ appId: string }>();
  return <AppDetailsPage appId={decodeRouteParam(appId)} />;
}

function UserDetailsRoute() {
  const { userKey } = useParams<{ userKey: string }>();
  return <UserDetailsPage userKey={decodeRouteParam(userKey)} />;
}

function PortDetailsRoute() {
  const { portUri } = useParams<{ portUri: string }>();
  return <PortDetailsPage portUri={decodeRouteParam(portUri)} />;
}

function MemoryExplorerRoute() {
  const { explorerUri } = useParams<{ explorerUri: string }>();
  const decoded = decodeRouteParam(explorerUri);
  return <MemoryExplorerPage explorerUri={decoded || undefined} />;
}

function MemoryEntityRoute() {
  const { entityUri } = useParams<{ entityUri: string }>();
  return <MemoryEntityPage entityUri={decodeRouteParam(entityUri)} />;
}

function TaskGraphTaskRoute() {
  const { taskUri } = useParams<{ taskUri: string }>();
  return <TaskGraphTaskDetailsPage taskUri={decodeRouteParam(taskUri)} />;
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

  { path: "control", element: <Navigate to="/control/sessions" replace /> },
  { path: "control/sessions", element: <SessionPage /> },
  { path: "control/sessions/:sessionId", element: <SessionDetailsRoute /> },
  { path: "control/behaviors", element: <BehaviorsPage /> },
  {
    path: "control/behaviors/:behaviorId",
    element: <BehaviorDetailsRoute />,
  },
  { path: "control/actors", element: <ActorsPage /> },
  { path: "control/actors/:actorId", element: <ActorDetailsRoute /> },
  { path: "control/apps", element: <AppsPage /> },
  { path: "control/apps/:appId", element: <AppDetailsRoute /> },
  { path: "control/clockwork", element: <ClockworkPage /> },
  { path: "clockwork", element: <Navigate to="/control/clockwork" replace /> },
  { path: "control/users", element: <UsersPage /> },
  { path: "control/users/:userKey", element: <UserDetailsRoute /> },
  { path: "control/ports", element: <PortsPage /> },
  { path: "control/ports/:portUri", element: <PortDetailsRoute /> },
  {
    path: "control/policies",
    element: (
      <p className="text-muted-foreground text-sm">Policies is coming next.</p>
    ),
  },

  { path: "memory", element: <Navigate to="/memory/explorer" replace /> },
  {
    path: "memory/search",
    element: <Navigate to="/memory/explorer" replace />,
  },
  { path: "memory/graph", element: <MemoryGraphPage /> },
  { path: "memory/explorer", element: <MemoryExplorerRoute /> },
  { path: "memory/explorer/:explorerUri", element: <MemoryExplorerRoute /> },
  { path: "memory/entity/:entityUri", element: <MemoryEntityRoute /> },

  { path: "taskgraph", element: <Navigate to="/taskgraph/explorer" replace /> },
  { path: "taskgraph/explorer", element: <TaskGraphExplorerPage /> },
  { path: "taskgraph/kanban", element: <TaskGraphKanbanPage /> },
  { path: "taskgraph/tasks/:taskUri", element: <TaskGraphTaskRoute /> },

  { path: "fs", element: <Navigate to="/fs/explorer" replace /> },
  { path: "fs/explorer", element: <FsExplorerPage /> },
  { path: "fs/settings", element: <FsSettingsPage /> },

  { path: "settings", element: <Navigate to="/settings/providers" replace /> },
  {
    path: "settings/provider",
    element: <Navigate to="/settings/providers" replace />,
  },
  { path: "settings/providers", element: <ProvidersPage /> },
  {
    path: "settings/provider/:providerId",
    element: <ProviderDetailsRoute />,
  },
  {
    path: "settings/providers/:providerId",
    element: <ProviderDetailsRoute />,
  },

  {
    path: "observability",
    element: <ObservabilityOverviewPage />,
  },
  {
    path: "observability/overview",
    element: <Navigate to="/observability" replace />,
  },
  { path: "observability/alerts", element: <ObservabilityAlertsPage /> },
  {
    path: "observability/traces",
    element: <Navigate to="/observability/tracing" replace />,
  },
  { path: "observability/tracing", element: <ObservabilityTracingPage /> },
  {
    path: "observability/tracing/llm-calls",
    element: <ObservabilityLlmCallsPage />,
  },
  {
    path: "observability/tracing/llm-calls/:callId",
    element: <ObservabilityLlmCallsPage />,
  },
  {
    path: "observability/tracing/tool-calls",
    element: <ObservabilityToolCallsPage />,
  },
  {
    path: "observability/tracing/tool-calls/:callId",
    element: <ObservabilityToolCallsPage />,
  },

  { path: "*", element: <Navigate to="/" replace /> },
];

type SectionMatcher = {
  id: string;
  path: string;
  end?: boolean;
};

const SECTION_MATCHERS: SectionMatcher[] = [
  { id: "control-sessions", path: "/control/sessions/:sessionId" },
  { id: "control-sessions", path: "/control/sessions", end: true },
  { id: "control-behaviors", path: "/control/behaviors/:behaviorId" },
  { id: "control-behaviors", path: "/control/behaviors", end: true },
  { id: "control-actors", path: "/control/actors/:actorId" },
  { id: "control-actors", path: "/control/actors", end: true },
  { id: "control-apps", path: "/control/apps/:appId" },
  { id: "control-apps", path: "/control/apps", end: true },
  { id: "control-clockwork", path: "/control/clockwork", end: true },
  { id: "control-users", path: "/control/users/:userKey" },
  { id: "control-users", path: "/control/users", end: true },
  { id: "control-ports", path: "/control/ports/:portUri" },
  { id: "control-ports", path: "/control/ports", end: true },
  { id: "control-policies", path: "/control/policies", end: true },

  { id: "memory-explorer", path: "/memory/entity/:entityUri" },
  { id: "memory-explorer", path: "/memory/explorer/:explorerUri" },
  { id: "memory-explorer", path: "/memory/explorer", end: true },
  { id: "memory-graph", path: "/memory/graph", end: true },

  { id: "taskgraph-explorer", path: "/taskgraph/tasks/:taskUri" },
  { id: "taskgraph-explorer", path: "/taskgraph/explorer", end: true },
  { id: "taskgraph-kanban", path: "/taskgraph/kanban", end: true },

  { id: "fs-explorer", path: "/fs/explorer", end: true },
  { id: "fs-settings", path: "/fs/settings", end: true },

  { id: "settings-providers", path: "/settings/providers/:providerId" },
  { id: "settings-providers", path: "/settings/provider/:providerId" },
  { id: "settings-providers", path: "/settings/providers", end: true },

  {
    id: "observability-tracing-llm-calls",
    path: "/observability/tracing/llm-calls/:callId",
  },
  {
    id: "observability-tracing-llm-calls",
    path: "/observability/tracing/llm-calls",
    end: true,
  },
  {
    id: "observability-tracing-tool-calls",
    path: "/observability/tracing/tool-calls/:callId",
  },
  {
    id: "observability-tracing-tool-calls",
    path: "/observability/tracing/tool-calls",
    end: true,
  },
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
    if (
      matchPath({ path: matcher.path, end: matcher.end ?? false }, pathname)
    ) {
      return matcher.id;
    }
  }

  if (matchPath({ path: "/control/*", end: false }, pathname)) {
    return "control-sessions";
  }
  if (matchPath({ path: "/memory/*", end: false }, pathname)) {
    return "memory-explorer";
  }
  if (matchPath({ path: "/taskgraph/*", end: false }, pathname)) {
    return "taskgraph-explorer";
  }
  if (matchPath({ path: "/fs/*", end: false }, pathname)) {
    return "fs-explorer";
  }
  if (matchPath({ path: "/settings/*", end: false }, pathname)) {
    return "settings-providers";
  }
  if (matchPath({ path: "/observability/*", end: false }, pathname)) {
    return "observability-overview";
  }

  return DEFAULT_SECTION_ID;
}
