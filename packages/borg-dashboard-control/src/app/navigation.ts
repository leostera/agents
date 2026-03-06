import {
  Bell,
  LayoutDashboard,
  Route,
  Settings2,
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
