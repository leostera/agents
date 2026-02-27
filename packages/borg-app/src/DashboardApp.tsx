import { BorgApiError, createBorgApiClient } from "@borg/api";
import { createI18n } from "@borg/i18n";
import { Button, SidebarInset, SidebarProvider } from "@borg/ui";
import {
  Bell,
  Bot,
  LayoutDashboard,
  Map,
  Plus,
  Route,
  ScanSearch,
  Settings2,
  Shield,
  Users,
  Workflow,
} from "lucide-react";
import React, { useEffect, useMemo, useState } from "react";
import { AppSidebar } from "./AppSidebar";
import { CommandK, type CommandSectionGroup } from "./CommandK";
import { SessionPage } from "./pages/control/sessions";
import { SessionDetailsPage } from "./pages/control/sessions/id";
import { MemoryEntityPage } from "./pages/memory/entity";
import { MemoryExplorerPage } from "./pages/memory/explorer";
import { MemoryGraphPage } from "./pages/memory/graph";
import { ObservabilityOverviewPage } from "./pages/observability";
import { ObservabilityAlertsPage } from "./pages/observability/alerts";
import { ObservabilityTracingPage } from "./pages/observability/tracing";
import { OverviewPage } from "./pages/overview";
import { ProvidersPage } from "./pages/settings/providers";

type DashboardRouteItem = CommandSectionGroup["items"][number] & {
  path: string;
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
};

const SECTION_GROUPS: DashboardRouteGroup[] = [
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
        id: "control-agents",
        title: "Agents",
        icon: Bot,
        path: "/control/agents",
      },
      {
        id: "control-users",
        title: "Users",
        icon: Users,
        path: "/control/users",
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
      },
    ],
  },
];

const ALL_SECTIONS = SECTION_GROUPS.flatMap((group) => group.items);
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
  "/settings": SECTION_BY_ID["settings-providers"],
  "/observability/overview": SECTION_BY_ID["observability-overview"],
  "/observability/traces": SECTION_BY_ID["observability-tracing"],
  "/observability": SECTION_BY_ID["observability-overview"],
  "/memory": SECTION_BY_ID["memory-explorer"],
  "/memory/search": SECTION_BY_ID["memory-explorer"],
};
const MEMORY_ENTITY_PREFIX = "/memory/entity/";
const MEMORY_EXPLORER_PREFIX = "/memory/explorer/";
const CONTROL_SESSION_PREFIX = "/control/sessions/";
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
      };
    } catch {
      return {
        id: "memory-entity",
        entityUri: encodedUri,
        explorerUri: null,
        sessionId: null,
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
      };
    } catch {
      return {
        id: "memory-explorer",
        entityUri: null,
        explorerUri: encodedUri,
        sessionId: null,
      };
    }
  }
  if (
    normalizedPathname.startsWith(CONTROL_SESSION_PREFIX) &&
    normalizedPathname.length > CONTROL_SESSION_PREFIX.length
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
      };
    } catch {
      return {
        id: "control-session",
        entityUri: null,
        explorerUri: null,
        sessionId: encodedSessionId,
      };
    }
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
      };
    return resolveRouteFromPath(window.location.pathname);
  });
  const activeId = route.id;
  const [isCommandMenuOpen, setIsCommandMenuOpen] = useState(false);
  const [isOffline, setIsOffline] = useState(false);
  const i18n = useMemo(() => createI18n("en"), []);
  const username = useMemo(resolveUsername, []);
  const initials = useMemo(() => initialsFromUsername(username), [username]);

  const ActiveSection = useMemo(() => {
    const sectionById: Record<string, () => React.JSX.Element> = {
      "overview-home": () => <OverviewPage />,
      "control-sessions": () => <SessionPage />,
      "control-session": () => (
        <SessionDetailsPage sessionId={route.sessionId ?? ""} />
      ),
      "control-agents": () => <ControlPlaceholder title="Agents" />,
      "control-users": () => <ControlPlaceholder title="Users" />,
      "control-policies": () => <ControlPlaceholder title="Policies" />,
      "observability-overview": () => <ObservabilityOverviewPage />,
      "observability-alerts": () => <ObservabilityAlertsPage />,
      "observability-tracing": () => <ObservabilityTracingPage />,
      "memory-graph": () => <MemoryGraphPage />,
      "memory-explorer": () => (
        <MemoryExplorerPage explorerUri={route.explorerUri ?? undefined} />
      ),
      "memory-entity": () => (
        <MemoryEntityPage entityUri={route.entityUri ?? ""} />
      ),
      "settings-providers": () => <ProvidersPage />,
    };
    return sectionById[activeId] ?? sectionById["overview-home"];
  }, [activeId, route.entityUri, route.explorerUri, route.sessionId]);

  const activeTitle = useMemo(() => {
    if (activeId === "memory-entity") return "Entity";
    if (activeId === "control-session") return "Session";
    const section = ALL_SECTIONS.find((item) => item.id === activeId);
    return section?.title ?? "Overview";
  }, [activeId]);

  const activeSubtitle = useMemo(() => {
    if (activeId === "settings-providers") {
      return i18n.t("dashboard.subtitle.settings.providers");
    }
    if (activeId === "memory-entity") {
      return route.entityUri ?? i18n.t("dashboard.subtitle.default");
    }
    if (activeId === "control-session") {
      return route.sessionId ?? i18n.t("dashboard.subtitle.default");
    }
    return i18n.t("dashboard.subtitle.default");
  }, [activeId, i18n, route.entityUri, route.sessionId]);

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
    });
    if (section && window.location.pathname !== section.path) {
      window.history.pushState(null, "", section.path);
    }
    setIsCommandMenuOpen(false);
  };

  const headlineActions = useMemo(() => {
    if (activeId === "settings-providers") {
      return (
        <Button
          variant="outline"
          onClick={() =>
            window.dispatchEvent(new CustomEvent("providers:open-connect"))
          }
        >
          <Plus className="size-4" />
          Connect Provider
        </Button>
      );
    }
    return null;
  }, [activeId]);

  const isExplorerImmersive = activeId === "memory-graph";
  const isHeadlineHidden =
    activeId === "memory-graph" || activeId === "memory-explorer";

  return (
    <section className="borg-dashboard-shell text-foreground">
      <SidebarProvider defaultOpen>
        <AppSidebar
          activeId={activeId}
          onSelect={handleSelectSection}
          onOpenCommandMenu={() => setIsCommandMenuOpen(true)}
          groups={SECTION_GROUPS}
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
            <section
              id={activeId}
              className={
                isExplorerImmersive ? "borg-dashboard-section--full" : undefined
              }
            >
              <ActiveSection />
            </section>
          </div>
        </SidebarInset>
      </SidebarProvider>
      <CommandK
        open={isCommandMenuOpen}
        onOpenChange={setIsCommandMenuOpen}
        groups={SECTION_GROUPS}
        onSelectSection={handleSelectSection}
      />
    </section>
  );
}
