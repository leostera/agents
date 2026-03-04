import { BorgApiError, createBorgApiClient } from "@borg/api";
import { SidebarInset, SidebarProvider } from "@borg/ui";
import React, { useEffect, useMemo, useState } from "react";
import { Outlet, useLocation, useNavigate } from "react-router-dom";
import { AppSidebar } from "../AppSidebar";
import { CommandK, type CommandSectionGroup } from "../CommandK";
import {
  type DashboardRouteItem,
  flattenRouteItems,
  NAVIGATION_GROUPS,
  SECTION_BY_ID,
} from "./navigation";
import { resolveActiveSectionId } from "./routing";

const HEALTH_CHECK_INTERVAL_MS = 15_000;
const borgApi = createBorgApiClient();

function resolveUsername(search: string): string {
  const fromQuery = new URLSearchParams(search).get("user")?.trim();
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

function routePathForSection(sectionId: string): string {
  const section = SECTION_BY_ID[sectionId];
  if (!section) return "/";
  return section.path;
}

export function DashboardLayout() {
  const location = useLocation();
  const navigate = useNavigate();
  const [isCommandMenuOpen, setIsCommandMenuOpen] = useState(false);
  const [isOffline, setIsOffline] = useState(false);

  const activeId = useMemo(
    () => resolveActiveSectionId(location.pathname),
    [location.pathname]
  );
  const username = useMemo(
    () => resolveUsername(location.search),
    [location.search]
  );
  const initials = useMemo(() => initialsFromUsername(username), [username]);

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
          }, HEALTH_CHECK_INTERVAL_MS);
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
    const targetPath = routePathForSection(sectionId);
    if (location.pathname !== targetPath) {
      navigate(targetPath);
    }
    setIsCommandMenuOpen(false);
  };

  const isExplorerImmersive = activeId === "memory-graph";
  const commandGroups = React.useMemo<CommandSectionGroup[]>(
    () =>
      NAVIGATION_GROUPS.map((group) => ({
        id: group.id,
        title: group.title,
        items: flattenRouteItems(group.items).map(
          ({ id, title, icon }: DashboardRouteItem) => ({
            id,
            title,
            icon,
          })
        ),
      })),
    []
  );

  return (
    <section className="borg-dashboard-shell text-foreground">
      <SidebarProvider defaultOpen>
        <AppSidebar
          activeId={activeId}
          onSelectSection={handleSelectSection}
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
              <Outlet />
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
