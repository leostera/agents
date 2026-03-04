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
import React from "react";

export type DashboardRouteItem = {
  id: string;
  title: string;
  icon: React.ComponentType<{ className?: string }>;
  path: string;
  children?: DashboardRouteItem[];
};

export type DashboardRouteGroup = {
  id: string;
  title: string;
  items: DashboardRouteItem[];
};

export const SECTION_GROUPS: DashboardRouteGroup[] = [
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

export const NAVIGATION_GROUPS = SECTION_GROUPS.filter(
  (group) => group.id !== "overview"
);

export function flattenRouteItems(
  items: DashboardRouteItem[]
): DashboardRouteItem[] {
  return items.flatMap((item) =>
    item.children && item.children.length > 0
      ? [item, ...flattenRouteItems(item.children)]
      : [item]
  );
}

export const ALL_SECTIONS = SECTION_GROUPS.flatMap((group) =>
  flattenRouteItems(group.items)
);

export const SECTION_BY_ID = Object.fromEntries(
  ALL_SECTIONS.map((section) => [section.id, section])
) as Record<string, DashboardRouteItem>;

export const DEFAULT_SECTION_ID = "overview-home";
