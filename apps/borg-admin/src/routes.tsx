import {
  DashboardLayout,
  dashboardControlRoutes,
} from "@borg/dashboard-control";
import { OnboardingApp } from "@borg/onboarding";
import { TooltipProvider } from "@borg/ui";
import React from "react";
import { createBrowserRouter } from "react-router-dom";

function WithTooltip({ children }: { children: React.ReactNode }) {
  return <TooltipProvider>{children}</TooltipProvider>;
}

export const router: ReturnType<typeof createBrowserRouter> =
  createBrowserRouter([
    {
      path: "/onboard",
      element: (
        <WithTooltip>
          <OnboardingApp />
        </WithTooltip>
      ),
    },
    {
      path: "/",
      element: (
        <WithTooltip>
          <DashboardLayout />
        </WithTooltip>
      ),
      children: dashboardControlRoutes,
    },
  ]);
