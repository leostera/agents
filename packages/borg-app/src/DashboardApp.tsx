import { BorgApiError, createBorgApiClient } from "@borg/api";
import { SidebarInset, SidebarProvider } from "@borg/ui";
import {
  Bell,
  Bot,
  Brain,
  Clock3,
  FolderTree,
  GitFork,
  Hammer,
  LayoutDashboard,
  Map,
  Route,
  ScanSearch,
  Settings2,
  Shield,
  SlidersHorizontal,
  Users,
  Workflow,
} from "lucide-react";
import React, { useEffect, useMemo, useState } from "react";
import { AppSidebar } from "./AppSidebar";
import { CommandK, type CommandSectionGroup } from "./CommandK";
import { ActorsPage } from "./pages/control/actors";
import { ActorDetailsPage } from "./pages/control/actors/id";
import { AppsPage } from "./pages/control/apps";
import { AppDetailsPage } from "./pages/control/apps/id";
import { BehaviorsPage } from "./pages/control/behaviors";
import { BehaviorDetailsPage } from "./pages/control/behaviors/id";
import { ClockworkPage } from "./pages/control/clockwork";
import { PortsPage } from "./pages/control/ports";
import { PortDetailsPage } from "./pages/control/ports/id";
import { SessionPage } from "./pages/control/sessions";
import { SessionDetailsPage } from "./pages/control/sessions/id";
import { UsersPage } from "./pages/control/users";
import { UserDetailsPage } from "./pages/control/users/id";
import { FsExplorerPage } from "./pages/fs/explorer";
import { FsSettingsPage } from "./pages/fs/settings";
import { MemoryEntityPage } from "./pages/memory/entity";
import { MemoryExplorerPage } from "./pages/memory/explorer";
import { MemoryGraphPage } from "./pages/memory/graph";
import { ObservabilityOverviewPage } from "./pages/observability";
import { ObservabilityAlertsPage } from "./pages/observability/alerts";
import { ObservabilityTracingPage } from "./pages/observability/tracing";
import { OverviewPage } from "./pages/overview";
import { ProvidersPage } from "./pages/settings/providers";
import { ProviderDetailsPage } from "./pages/settings/providers/id";
import { TaskGraphExplorerPage } from "./pages/taskgraph/explorer";
import { TaskGraphKanbanPage } from "./pages/taskgraph/kanban";
import { TaskGraphTaskDetailsPage } from "./pages/taskgraph/task";

type DashboardRouteItem = {
  id: string;
  title: string;
  icon: React.ComponentType<{ className?: string }>;
  path: string;
  children?: DashboardRouteItem[];
};

type DashboardRouteGroup = {
  id: string;
  title: string;
  items: DashboardRouteItem[];
};

type ResolvedDashboardRoute = {
  id: string;
  entityUri: string | null;
  explorerUri: string | null;
  sessionId: string | null;
  portName: string | null;
};

const SECTION_GROUPS: DashboardRouteGroup[] = [
  {
    id: "control",
    title: "Control",
    items: [
      {
        id: "control-sessions",
        title: "Sessions",
        icon: Workflow,
        path: "/control/sessions",
      },
      {
        id: "control-behaviors",
        title: "Behaviors",
        icon: Brain,
        path: "/control/behaviors",
      },
      {
        id: "control-actors",
        title: "Actors",
        icon: Bot,
        path: "/control/actors",
      },
      {
        id: "control-apps",
        title: "Apps",
        icon: Hammer,
        path: "/control/apps",
      },
      {
        id: "control-clockwork",
        title: "Clockwork",
        icon: Clock3,
        path: "/control/clockwork",
      },
      {
        id: "control-users",
        title: "Users",
        icon: Users,
        path: "/control/users",
      },
      {
        id: "control-ports",
        title: "Ports",
        icon: Route,
        path: "/control/ports",
      },
      {
        id: "control-policies",
        title: "Policies",
        icon: Shield,
        path: "/control/policies",
      },
    ],
  },
  {
    id: "memory",
    title: "Memory",
    items: [
      {
        id: "memory-explorer",
        title: "Explorer",
        icon: ScanSearch,
        path: "/memory/explorer",
      },
      {
        id: "memory-graph",
        title: "Graph",
        icon: Map,
        path: "/memory/graph",
      },
    ],
  },
  {
    id: "taskgraph",
    title: "Task Graph",
    items: [
      {
        id: "taskgraph-explorer",
        title: "Explorer",
        icon: GitFork,
        path: "/taskgraph/explorer",
      },
      {
        id: "taskgraph-kanban",
        title: "Kanban",
        icon: Workflow,
        path: "/taskgraph/kanban",
      },
    ],
  },
  {
    id: "fs",
    title: "FS",
    items: [
      {
        id: "fs-explorer",
        title: "Explorer",
        icon: FolderTree,
        path: "/fs/explorer",
      },
      {
        id: "fs-settings",
        title: "Settings",
        icon: SlidersHorizontal,
        path: "/fs/settings",
      },
    ],
  },
  {
    id: "settings",
    title: "Settings",
    items: [
      {
        id: "settings-providers",
        title: "Providers",
        icon: Settings2,
        path: "/settings/providers",
      },
    ],
  },
  {
    id: "observability",
    title: "Observability",
    items: [
      {
        id: "observability-overview",
        title: "Overview",
        icon: LayoutDashboard,
        path: "/observability",
      },
      {
        id: "observability-alerts",
        title: "Alerts",
        icon: Bell,
        path: "/observability/alerts",
      },
      {
        id: "observability-tracing",
        title: "Tracing",
        icon: Route,
        path: "/observability/tracing",
        children: [
          {
            id: "observability-tracing-llm-calls",
            title: "LLM Calls",
            icon: Route,
            path: "/observability/tracing/llm-calls",
          },
          {
            id: "observability-tracing-tool-calls",
            title: "Tool Calls",
            icon: Route,
            path: "/observability/tracing/tool-calls",
          },
        ],
      },
    ],
  },
  {
    id: "overview",
    title: "Overview",
    items: [
      {
        id: "overview-home",
        title: "Overview",
        icon: LayoutDashboard,
        path: "/",
      },
    ],
  },
];
const NAVIGATION_GROUPS = SECTION_GROUPS.filter(
  (group) => group.id !== "overview"
);

function flattenRouteItems(items: DashboardRouteItem[]): DashboardRouteItem[] {
  return items.flatMap((item) =>
    item.children && item.children.length > 0
      ? [item, ...flattenRouteItems(item.children)]
      : [item]
  );
}

const ALL_SECTIONS = SECTION_GROUPS.flatMap((group) =>
  flattenRouteItems(group.items)
);
const SECTION_BY_ID = Object.fromEntries(
  ALL_SECTIONS.map((section) => [section.id, section])
) as Record<string, DashboardRouteItem>;
const SECTION_BY_PATH = Object.fromEntries(
  ALL_SECTIONS.map((section) => [section.path, section])
) as Record<string, DashboardRouteItem>;
const SECTION_BY_PATH_ALIASES: Record<string, DashboardRouteItem> = {
  "/overview": SECTION_BY_ID["overview-home"],
  "/dashboard": SECTION_BY_ID["overview-home"],
  "/dashbaord": SECTION_BY_ID["overview-home"],
  "/control": SECTION_BY_ID["control-sessions"],
  "/clockwork": SECTION_BY_ID["control-clockwork"],
  "/control/tools": SECTION_BY_ID["control-apps"],
  "/settings": SECTION_BY_ID["settings-providers"],
  "/fs": SECTION_BY_ID["fs-explorer"],
  "/observability/overview": SECTION_BY_ID["observability-overview"],
  "/observability/traces": SECTION_BY_ID["observability-tracing"],
  "/observability/tracing/llm-calls":
    SECTION_BY_ID["observability-tracing-llm-calls"],
  "/observability/tracing/tool-calls":
    SECTION_BY_ID["observability-tracing-tool-calls"],
  "/observability": SECTION_BY_ID["observability-overview"],
  "/memory": SECTION_BY_ID["memory-explorer"],
  "/memory/search": SECTION_BY_ID["memory-explorer"],
  "/taskgraph": SECTION_BY_ID["taskgraph-explorer"],
};
const MEMORY_ENTITY_PREFIX = "/memory/entity/";
const MEMORY_EXPLORER_PREFIX = "/memory/explorer/";
const CONTROL_SESSION_PREFIX = "/control/sessions/";
const CONTROL_BEHAVIOR_PREFIX = "/control/behaviors/";
const CONTROL_ACTOR_PREFIX = "/control/actors/";
const CONTROL_APP_PREFIX = "/control/apps/";
const CONTROL_USER_PREFIX = "/control/users/";
const CONTROL_PORT_PREFIX = "/control/ports/";
const OBSERVABILITY_TRACING_PREFIX = "/observability/tracing/";
const SETTINGS_PROVIDER_PREFIX = "/settings/providers/";
const SETTINGS_PROVIDER_LEGACY_PREFIX = "/settings/provider/";
const TASKGRAPH_TASK_PREFIX = "/taskgraph/tasks/";
const borgApi = createBorgApiClient();

function ControlPlaceholder({ title }: { title: string }) {
  return (
    <section className="space-y-2">
      <p className="text-muted-foreground text-sm">{title} is coming next.</p>
    </section>
  );
}

function normalizePathname(pathname: string): string {
  return pathname.replace(/\/+$/, "") || "/";
}

function resolveRouteFromPath(pathname: string): ResolvedDashboardRoute {
  const normalizedPathname = normalizePathname(pathname);
  if (
    normalizedPathname.startsWith(MEMORY_ENTITY_PREFIX) &&
    normalizedPathname.length > MEMORY_ENTITY_PREFIX.length
  ) {
    const encodedUri = normalizedPathname.slice(MEMORY_ENTITY_PREFIX.length);
    try {
      return {
        id: "memory-entity",
        entityUri: decodeURIComponent(encodedUri),
        explorerUri: null,
        sessionId: null,
        portName: null,
      };
    } catch {
      return {
        id: "memory-entity",
        entityUri: encodedUri,
        explorerUri: null,
        sessionId: null,
        portName: null,
      };
    }
  }
  if (
    normalizedPathname.startsWith(MEMORY_EXPLORER_PREFIX) &&
    normalizedPathname.length > MEMORY_EXPLORER_PREFIX.length
  ) {
    const encodedUri = normalizedPathname.slice(MEMORY_EXPLORER_PREFIX.length);
    try {
      return {
        id: "memory-explorer",
        entityUri: null,
        explorerUri: decodeURIComponent(encodedUri),
        sessionId: null,
        portName: null,
      };
    } catch {
      return {
        id: "memory-explorer",
        entityUri: null,
        explorerUri: encodedUri,
        sessionId: null,
        portName: null,
      };
    }
  }
  if (
    normalizedPathname.startsWith(CONTROL_SESSION_PREFIX) &&
    normalizedPathname.length > CONTROL_SESSION_PREFIX.length &&
    !SECTION_BY_PATH[normalizedPathname]
  ) {
    const encodedSessionId = normalizedPathname.slice(
      CONTROL_SESSION_PREFIX.length
    );
    try {
      return {
        id: "control-session",
        entityUri: null,
        explorerUri: null,
        sessionId: decodeURIComponent(encodedSessionId),
        portName: null,
      };
    } catch {
      return {
        id: "control-session",
        entityUri: null,
        explorerUri: null,
        sessionId: encodedSessionId,
        portName: null,
      };
    }
  }
  if (
    normalizedPathname.startsWith(CONTROL_BEHAVIOR_PREFIX) &&
    normalizedPathname.length > CONTROL_BEHAVIOR_PREFIX.length &&
    !SECTION_BY_PATH[normalizedPathname]
  ) {
    const encodedBehaviorId = normalizedPathname.slice(
      CONTROL_BEHAVIOR_PREFIX.length
    );
    try {
      return {
        id: "control-behavior",
        entityUri: decodeURIComponent(encodedBehaviorId),
        explorerUri: null,
        sessionId: null,
        portName: null,
      };
    } catch {
      return {
        id: "control-behavior",
        entityUri: encodedBehaviorId,
        explorerUri: null,
        sessionId: null,
        portName: null,
      };
    }
  }
  if (
    normalizedPathname.startsWith(CONTROL_ACTOR_PREFIX) &&
    normalizedPathname.length > CONTROL_ACTOR_PREFIX.length &&
    !SECTION_BY_PATH[normalizedPathname]
  ) {
    const encodedActorId = normalizedPathname.slice(
      CONTROL_ACTOR_PREFIX.length
    );
    try {
      return {
        id: "control-actor",
        entityUri: decodeURIComponent(encodedActorId),
        explorerUri: null,
        sessionId: null,
        portName: null,
      };
    } catch {
      return {
        id: "control-actor",
        entityUri: encodedActorId,
        explorerUri: null,
        sessionId: null,
        portName: null,
      };
    }
  }
  if (
    normalizedPathname.startsWith(CONTROL_APP_PREFIX) &&
    normalizedPathname.length > CONTROL_APP_PREFIX.length &&
    !SECTION_BY_PATH[normalizedPathname]
  ) {
    const encodedAppId = normalizedPathname.slice(CONTROL_APP_PREFIX.length);
    try {
      return {
        id: "control-app",
        entityUri: decodeURIComponent(encodedAppId),
        explorerUri: null,
        sessionId: null,
        portName: null,
      };
    } catch {
      return {
        id: "control-app",
        entityUri: encodedAppId,
        explorerUri: null,
        sessionId: null,
        portName: null,
      };
    }
  }
  if (
    normalizedPathname.startsWith(CONTROL_PORT_PREFIX) &&
    normalizedPathname.length > CONTROL_PORT_PREFIX.length &&
    !SECTION_BY_PATH[normalizedPathname]
  ) {
    const encodedPort = normalizedPathname.slice(CONTROL_PORT_PREFIX.length);
    try {
      return {
        id: "control-port",
        entityUri: null,
        explorerUri: null,
        sessionId: null,
        portName: decodeURIComponent(encodedPort),
      };
    } catch {
      return {
        id: "control-port",
        entityUri: null,
        explorerUri: null,
        sessionId: null,
        portName: encodedPort,
      };
    }
  }
  if (
    normalizedPathname.startsWith(CONTROL_USER_PREFIX) &&
    normalizedPathname.length > CONTROL_USER_PREFIX.length &&
    !SECTION_BY_PATH[normalizedPathname]
  ) {
    const encodedUserKey = normalizedPathname.slice(CONTROL_USER_PREFIX.length);
    try {
      return {
        id: "control-user",
        entityUri: null,
        explorerUri: decodeURIComponent(encodedUserKey),
        sessionId: null,
        portName: null,
      };
    } catch {
      return {
        id: "control-user",
        entityUri: null,
        explorerUri: encodedUserKey,
        sessionId: null,
        portName: null,
      };
    }
  }
  if (
    normalizedPathname.startsWith("/observability/tracing/llm-calls/") &&
    normalizedPathname.length > "/observability/tracing/llm-calls/".length
  ) {
    return {
      id: "observability-tracing-llm-calls",
      entityUri: null,
      explorerUri: null,
      sessionId: null,
      portName: null,
    };
  }
  if (
    normalizedPathname.startsWith("/observability/tracing/tool-calls/") &&
    normalizedPathname.length > "/observability/tracing/tool-calls/".length
  ) {
    return {
      id: "observability-tracing-tool-calls",
      entityUri: null,
      explorerUri: null,
      sessionId: null,
      portName: null,
    };
  }
  if (
    normalizedPathname.startsWith(OBSERVABILITY_TRACING_PREFIX) &&
    normalizedPathname.length > OBSERVABILITY_TRACING_PREFIX.length &&
    !SECTION_BY_PATH[normalizedPathname]
  ) {
    return {
      id: "observability-tracing",
      entityUri: null,
      explorerUri: null,
      sessionId: null,
      portName: null,
    };
  }
  if (
    normalizedPathname.startsWith(TASKGRAPH_TASK_PREFIX) &&
    normalizedPathname.length > TASKGRAPH_TASK_PREFIX.length
  ) {
    const encodedTaskUri = normalizedPathname.slice(
      TASKGRAPH_TASK_PREFIX.length
    );
    try {
      return {
        id: "taskgraph-task",
        entityUri: decodeURIComponent(encodedTaskUri),
        explorerUri: null,
        sessionId: null,
        portName: null,
      };
    } catch {
      return {
        id: "taskgraph-task",
        entityUri: encodedTaskUri,
        explorerUri: null,
        sessionId: null,
        portName: null,
      };
    }
  }
  if (
    normalizedPathname.startsWith(SETTINGS_PROVIDER_PREFIX) &&
    normalizedPathname.length > SETTINGS_PROVIDER_PREFIX.length
  ) {
    const encodedProvider = normalizedPathname.slice(
      SETTINGS_PROVIDER_PREFIX.length
    );
    try {
      return {
        id: "settings-provider",
        entityUri: decodeURIComponent(encodedProvider),
        explorerUri: null,
        sessionId: null,
        portName: null,
      };
    } catch {
      return {
        id: "settings-provider",
        entityUri: encodedProvider,
        explorerUri: null,
        sessionId: null,
        portName: null,
      };
    }
  }
  if (
    normalizedPathname.startsWith(SETTINGS_PROVIDER_LEGACY_PREFIX) &&
    normalizedPathname.length > SETTINGS_PROVIDER_LEGACY_PREFIX.length
  ) {
    const encodedProvider = normalizedPathname.slice(
      SETTINGS_PROVIDER_LEGACY_PREFIX.length
    );
    try {
      return {
        id: "settings-provider",
        entityUri: decodeURIComponent(encodedProvider),
        explorerUri: null,
        sessionId: null,
        portName: null,
      };
    } catch {
      return {
        id: "settings-provider",
        entityUri: encodedProvider,
        explorerUri: null,
        sessionId: null,
        portName: null,
      };
    }
  }
  if (
    normalizedPathname === "/settings/provider" ||
    normalizedPathname === "/settings/providers"
  ) {
    return {
      id: "settings-providers",
      entityUri: null,
      explorerUri: null,
      sessionId: null,
      portName: null,
    };
  }

  const section =
    SECTION_BY_PATH[normalizedPathname] ??
    SECTION_BY_PATH_ALIASES[normalizedPathname] ??
    SECTION_BY_ID["overview-home"];
  return {
    id: section.id,
    entityUri: null,
    explorerUri: null,
    sessionId: null,
    portName: null,
  };
}

function resolveUsername(): string {
  if (typeof window === "undefined") return "friend";
  const fromQuery = new URLSearchParams(window.location.search)
    .get("user")
    ?.trim();
  return fromQuery && fromQuery.length > 0 ? fromQuery : "friend";
}

function initialsFromUsername(username: string): string {
  return (
    username
      .split(/\s+/)
      .filter(Boolean)
      .slice(0, 2)
      .map((part) => part[0]?.toUpperCase() ?? "")
      .join("") || "U"
  );
}

export function DashboardApp() {
  const [route, setRoute] = useState<ResolvedDashboardRoute>(() => {
    if (typeof window === "undefined")
      return {
        id: ALL_SECTIONS[0].id,
        entityUri: null,
        explorerUri: null,
        sessionId: null,
        portName: null,
      };
    return resolveRouteFromPath(window.location.pathname);
  });
  const activeId = route.id;
  const [isCommandMenuOpen, setIsCommandMenuOpen] = useState(false);
  const [isOffline, setIsOffline] = useState(false);
  const username = useMemo(resolveUsername, []);
  const initials = useMemo(() => initialsFromUsername(username), [username]);

  const ActiveSection = useMemo(() => {
    const sectionById: Record<string, () => React.JSX.Element> = {
      "overview-home": () => <OverviewPage />,
      "control-sessions": () => <SessionPage />,
      "control-session": () => (
        <SessionDetailsPage sessionId={route.sessionId ?? ""} />
      ),
      "control-behaviors": () => <BehaviorsPage />,
      "control-behavior": () => (
        <BehaviorDetailsPage behaviorId={route.entityUri ?? ""} />
      ),
      "control-actors": () => <ActorsPage />,
      "control-actor": () => (
        <ActorDetailsPage actorId={route.entityUri ?? ""} />
      ),
      "control-apps": () => <AppsPage />,
      "control-clockwork": () => <ClockworkPage />,
      "control-app": () => <AppDetailsPage appId={route.entityUri ?? ""} />,
      "control-users": () => <UsersPage />,
      "control-user": () => (
        <UserDetailsPage userKey={route.explorerUri ?? ""} />
      ),
      "control-ports": () => <PortsPage />,
      "control-port": () => <PortDetailsPage portUri={route.portName ?? ""} />,
      "control-policies": () => <ControlPlaceholder title="Policies" />,
      "observability-overview": () => <ObservabilityOverviewPage />,
      "observability-alerts": () => <ObservabilityAlertsPage />,
      "observability-tracing": () => <ObservabilityTracingPage />,
      "observability-tracing-llm-calls": () => <ObservabilityTracingPage />,
      "observability-tracing-tool-calls": () => <ObservabilityTracingPage />,
      "memory-graph": () => <MemoryGraphPage />,
      "memory-explorer": () => (
        <MemoryExplorerPage explorerUri={route.explorerUri ?? undefined} />
      ),
      "memory-entity": () => (
        <MemoryEntityPage entityUri={route.entityUri ?? ""} />
      ),
      "taskgraph-explorer": () => <TaskGraphExplorerPage />,
      "taskgraph-kanban": () => <TaskGraphKanbanPage />,
      "taskgraph-task": () => (
        <TaskGraphTaskDetailsPage taskUri={route.entityUri ?? ""} />
      ),
      "fs-explorer": () => <FsExplorerPage />,
      "fs-settings": () => <FsSettingsPage />,
      "settings-providers": () => <ProvidersPage />,
      "settings-provider": () => (
        <ProviderDetailsPage providerId={route.entityUri ?? ""} />
      ),
    };
    return sectionById[activeId] ?? sectionById["overview-home"];
  }, [
    activeId,
    route.entityUri,
    route.explorerUri,
    route.portName,
    route.sessionId,
  ]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key.toLowerCase() !== "k") return;
      if (!event.metaKey && !event.ctrlKey) return;
      event.preventDefault();
      setIsCommandMenuOpen((open) => !open);
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

  useEffect(() => {
    const handlePopState = () => {
      setRoute(resolveRouteFromPath(window.location.pathname));
    };

    window.addEventListener("popstate", handlePopState);
    return () => window.removeEventListener("popstate", handlePopState);
  }, []);

  useEffect(() => {
    let isActive = true;
    let timeoutId: number | undefined;

    const checkConnectivity = async () => {
      try {
        const isHealthy = await borgApi.health();
        if (isActive) {
          setIsOffline(!isHealthy);
        }
      } catch (error) {
        if (!isActive) return;
        setIsOffline(
          !(error instanceof BorgApiError && typeof error.status === "number")
        );
      } finally {
        if (isActive) {
          timeoutId = window.setTimeout(() => {
            void checkConnectivity();
          }, 15000);
        }
      }
    };

    void checkConnectivity();
    return () => {
      isActive = false;
      if (timeoutId) window.clearTimeout(timeoutId);
    };
  }, []);

  const handleSelectSection = (sectionId: string) => {
    const section = SECTION_BY_ID[sectionId];
    setRoute({
      id: sectionId,
      entityUri: null,
      explorerUri: null,
      sessionId: null,
      portName: null,
    });
    if (section && window.location.pathname !== section.path) {
      window.history.pushState(null, "", section.path);
    }
    setIsCommandMenuOpen(false);
  };

  const isExplorerImmersive = activeId === "memory-graph";
  const isHeadlineHidden =
    activeId === "memory-graph" || activeId === "memory-explorer";
  const commandGroups = React.useMemo<CommandSectionGroup[]>(
    () =>
      NAVIGATION_GROUPS.map((group) => ({
        id: group.id,
        title: group.title,
        items: flattenRouteItems(group.items).map(({ id, title, icon }) => ({
          id,
          title,
          icon,
        })),
      })),
    []
  );

  return (
    <section className="borg-dashboard-shell text-foreground">
      <SidebarProvider defaultOpen>
        <AppSidebar
          activeId={activeId}
          onSelect={handleSelectSection}
          onOpenCommandMenu={() => setIsCommandMenuOpen(true)}
          groups={NAVIGATION_GROUPS}
          username={username}
          initials={initials}
        />
        <SidebarInset
          className={isExplorerImmersive ? "p-0 md:p-0" : "p-4 md:p-6"}
        >
          <div
            className={`borg-dashboard-content${isExplorerImmersive ? " borg-dashboard-content--full" : ""}`}
          >
            {isOffline ? (
              <section
                className={
                  isExplorerImmersive
                    ? "flex h-[52px] items-center border-b border-red-500/40 bg-red-500/15 px-4 text-sm text-red-700"
                    : "-mx-4 -mt-4 mb-4 flex h-[52px] items-center border-b border-red-500/40 bg-red-500/15 px-4 text-sm text-red-700 md:-mx-6 md:-mt-6 md:mb-6 md:px-6"
                }
              >
                Offline: Borg API is unreachable.
              </section>
            ) : null}
            <section id={activeId} className="borg-dashboard-section--full">
              <ActiveSection />
            </section>
          </div>
        </SidebarInset>
      </SidebarProvider>
      <CommandK
        open={isCommandMenuOpen}
        onOpenChange={setIsCommandMenuOpen}
        groups={commandGroups}
        onSelectSection={handleSelectSection}
      />
    </section>
  );
}
