import { createI18n } from "@borg/i18n";
import { Card, TooltipProvider } from "@borg/ui";
import React, { useMemo } from "react";
import { DashboardApp } from "./DashboardApp";

const ONBOARD_PATH = "/onboard";
const DASHBOARD_PATH = "/dashboard";
const DASHBOARD_TYPO_PATH = "/dashbaord";
const DASHBOARD_PREFIXES = [
  "/control",
  "/observability",
  "/memory",
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
        <section className="p-6">
          <Card title="Onboarding">
            <p className="text-sm text-muted-foreground">
              Onboarding is being rebuilt in `borg-app`.
            </p>
          </Card>
        </section>
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
