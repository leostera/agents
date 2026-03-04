import React from "react";
import { useRoutes } from "react-router-dom";
import { DashboardLayout } from "./DashboardLayout";
import { dashboardControlRoutes } from "./routing";

export function StandaloneDashboardControlApp() {
  return useRoutes([
    {
      path: "/",
      element: <DashboardLayout />,
      children: dashboardControlRoutes,
    },
  ]);
}
