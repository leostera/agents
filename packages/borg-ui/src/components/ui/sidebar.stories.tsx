import {
  BellIcon,
  BrainIcon,
  GearIcon,
  HouseIcon,
  MagnifyingGlassIcon,
  PlusIcon,
  RobotIcon,
  ShieldCheckIcon,
  StackIcon,
  UserCircleIcon,
} from "@phosphor-icons/react";
import type { Meta, StoryObj } from "@storybook/react";

import { Badge } from "./badge";
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupAction,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarInset,
  SidebarMenu,
  SidebarMenuBadge,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
  SidebarRail,
  SidebarSeparator,
  SidebarTrigger,
} from "./sidebar";

const meta: Meta<typeof Sidebar> = {
  title: "UI/Sidebar",
  component: Sidebar,
};

export default meta;
type Story = StoryObj<typeof Sidebar>;

function SidebarDemo({ defaultOpen = true }: { defaultOpen?: boolean }) {
  return (
    <div className="border rounded-xl overflow-hidden min-h-[540px]">
      <SidebarProvider defaultOpen={defaultOpen}>
        <Sidebar collapsible="icon" variant="inset">
          <SidebarHeader />
          <SidebarSeparator />
          <SidebarContent>
            <SidebarGroup>
              <SidebarGroupLabel>Workspace</SidebarGroupLabel>
              <SidebarGroupAction aria-label="Create">
                <PlusIcon />
              </SidebarGroupAction>
              <SidebarGroupContent>
                <SidebarMenu>
                  <SidebarMenuItem>
                    <SidebarMenuButton
                      tooltip="Search sessions"
                      className="h-8 w-8 justify-center p-0 data-[state=open]:ml-auto"
                    >
                      <MagnifyingGlassIcon />
                      <span className="sr-only">Search sessions</span>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                  <SidebarMenuItem>
                    <SidebarMenuButton isActive>
                      <HouseIcon />
                      <span>Overview</span>
                    </SidebarMenuButton>
                    <SidebarMenuBadge>4</SidebarMenuBadge>
                  </SidebarMenuItem>
                  <SidebarMenuItem>
                    <SidebarMenuButton>
                      <RobotIcon />
                      <span>Sessions</span>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                  <SidebarMenuItem>
                    <SidebarMenuButton>
                      <StackIcon />
                      <span>Tasks</span>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                  <SidebarMenuItem>
                    <SidebarMenuButton>
                      <GearIcon />
                      <span>Settings</span>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                </SidebarMenu>
              </SidebarGroupContent>
            </SidebarGroup>
          </SidebarContent>
          <SidebarFooter>
            <div className="min-w-0 w-full overflow-hidden border rounded-md p-2 text-xs/relaxed">
              <p className="truncate font-medium">Pro workspace</p>
              <p className="truncate text-muted-foreground">
                42 sessions this week
              </p>
            </div>
          </SidebarFooter>
          <SidebarRail />
        </Sidebar>
        <SidebarInset className="p-4 gap-3">
          <div className="flex items-center justify-between">
            <SidebarTrigger />
            <Badge variant="outline">session_0f9a</Badge>
          </div>
          <div className="border rounded-lg p-4 text-xs/relaxed space-y-1">
            <p className="font-medium">Session transcript</p>
            <p className="text-muted-foreground">
              Provider connected and waiting for the next user message.
            </p>
          </div>
        </SidebarInset>
      </SidebarProvider>
    </div>
  );
}

export const Expanded: Story = {
  render: () => <SidebarDemo />,
};

export const CollapsedIconMode: Story = {
  render: () => <SidebarDemo defaultOpen={false} />,
};

function AppShellSidebarDemo() {
  return (
    <div className="border rounded-xl overflow-hidden min-h-[640px]">
      <SidebarProvider defaultOpen>
        <Sidebar
          collapsible="none"
          variant="sidebar"
          className="border-r border-border/60"
        >
          <SidebarHeader className="space-y-3 p-3">
            <div className="flex items-center gap-2 rounded-lg border border-border/60 bg-gradient-to-br from-card to-muted/30 p-3">
              <div className="min-w-0 flex-1">
                <p className="text-[10px] uppercase tracking-[0.16em] text-muted-foreground">
                  Borg
                </p>
                <h1 className="truncate text-base font-semibold">Dashboard</h1>
              </div>
              <SidebarMenuButton
                tooltip="Search sections"
                className="h-8 w-8 justify-center p-0"
              >
                <MagnifyingGlassIcon />
                <span className="sr-only">Search sections</span>
              </SidebarMenuButton>
            </div>
          </SidebarHeader>
          <SidebarSeparator className="mx-3" />
          <SidebarContent className="px-1 pb-2">
            <SidebarGroup className="py-2">
              <SidebarGroupLabel className="px-2 text-[10px] uppercase tracking-[0.14em] text-muted-foreground">
                Overview
              </SidebarGroupLabel>
              <SidebarGroupContent>
                <SidebarMenu>
                  <SidebarMenuItem>
                    <SidebarMenuButton
                      isActive
                      className="h-9 justify-start rounded-lg text-[13px] font-medium"
                    >
                      <HouseIcon />
                      <span>Overview</span>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                </SidebarMenu>
              </SidebarGroupContent>
            </SidebarGroup>

            <SidebarGroup className="py-2">
              <SidebarGroupLabel className="px-2 text-[10px] uppercase tracking-[0.14em] text-muted-foreground">
                Observability
              </SidebarGroupLabel>
              <SidebarGroupContent>
                <SidebarMenu>
                  <SidebarMenuItem>
                    <SidebarMenuButton className="h-9 justify-start rounded-lg text-[13px] font-medium">
                      <ShieldCheckIcon />
                      <span>Overview</span>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                  <SidebarMenuItem>
                    <SidebarMenuButton className="h-9 justify-start rounded-lg text-[13px] font-medium">
                      <BellIcon />
                      <span>Alerts</span>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                  <SidebarMenuItem>
                    <SidebarMenuButton className="h-9 justify-start rounded-lg text-[13px] font-medium">
                      <StackIcon />
                      <span>Tracing</span>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                </SidebarMenu>
              </SidebarGroupContent>
            </SidebarGroup>

            <SidebarGroup className="py-2">
              <SidebarGroupLabel className="px-2 text-[10px] uppercase tracking-[0.14em] text-muted-foreground">
                Memory
              </SidebarGroupLabel>
              <SidebarGroupContent>
                <SidebarMenu>
                  <SidebarMenuItem>
                    <SidebarMenuButton className="h-9 justify-start rounded-lg text-[13px] font-medium">
                      <BrainIcon />
                      <span>Explorer</span>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                </SidebarMenu>
              </SidebarGroupContent>
            </SidebarGroup>
          </SidebarContent>
          <SidebarFooter className="p-3">
            <div className="flex min-w-0 w-full items-center gap-2 overflow-hidden rounded-lg border border-border/60 bg-muted/20 p-2">
              <UserCircleIcon className="size-7 text-muted-foreground" />
              <div className="min-w-0">
                <p className="truncate text-sm font-medium">friend</p>
                <p className="text-xs text-muted-foreground">Workspace owner</p>
              </div>
            </div>
          </SidebarFooter>
          <SidebarRail />
        </Sidebar>
        <SidebarInset className="p-6">
          <div className="space-y-3">
            <h2 className="text-base font-semibold">Dashboard Preview</h2>
            <p className="text-xs text-muted-foreground">
              Use this story to debug sidebar-only layout and styling.
            </p>
          </div>
        </SidebarInset>
      </SidebarProvider>
    </div>
  );
}

export const AppShell: Story = {
  render: () => <AppShellSidebarDemo />,
};
