import { DevModeApp } from "@borg/devmode";
import { createI18n } from "@borg/i18n";
import { OnboardApp } from "@borg/onboard";
import { TooltipProvider } from "@borg/ui";
import React, { useMemo } from "react";
import { DashboardApp } from "./DashboardApp";

const ONBOARD_PATH = "/onboard";
const DASHBOARD_PATH = "/dashboard";
const DASHBOARD_TYPO_PATH = "/dashbaord";
const DASHBOARD_PREFIXES = [
  "/control",
  "/clockwork",
  "/observability",
  "/memory",
  "/taskgraph",
  "/fs",
  "/settings",
  "/overview",
];

export function App() {
  const i18n = useMemo(() => createI18n("en"), []);
  const rawPathname = window.location.pathname;
  const pathname =
    rawPathname.length > 1 && rawPathname.endsWith("/")
      ? rawPathname.slice(0, -1)
      : rawPathname;

  if (pathname === ONBOARD_PATH) {
    return (
      <TooltipProvider>
        <OnboardApp />
      </TooltipProvider>
    );
  }

  if (pathname === "/devmode" || pathname.startsWith("/devmode/")) {
    return (
      <TooltipProvider>
        <DevModeApp />
      </TooltipProvider>
    );
  }

  if (
    pathname === "/" ||
    pathname === DASHBOARD_PATH ||
    pathname === DASHBOARD_TYPO_PATH ||
    DASHBOARD_PREFIXES.some(
      (prefix) => pathname === prefix || pathname.startsWith(`${prefix}/`)
    )
  ) {
    return (
      <TooltipProvider>
        <DashboardApp />
      </TooltipProvider>
    );
  }

  return (
    <TooltipProvider>
      <section className="card">
        <p className="notice-error">
          {i18n.t("web.unknown_route")}: {pathname}
        </p>
      </section>
    </TooltipProvider>
  );
}
