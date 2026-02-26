import type { Meta, StoryObj } from '@storybook/react'
import {
  GearIcon,
  HouseIcon,
  PlusIcon,
  RobotIcon,
  StackIcon,
} from '@phosphor-icons/react'

import { Badge } from './badge'
import { SidebarInput } from './sidebar'
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
} from './sidebar'

const meta: Meta<typeof Sidebar> = {
  title: 'UI/Sidebar',
  component: Sidebar,
}

export default meta
type Story = StoryObj<typeof Sidebar>

function SidebarDemo({ defaultOpen = true }: { defaultOpen?: boolean }) {
  return (
    <div className='border rounded-xl overflow-hidden min-h-[540px]'>
      <SidebarProvider defaultOpen={defaultOpen}>
        <Sidebar collapsible='icon' variant='inset'>
          <SidebarHeader>
            <SidebarInput placeholder='Search sessions...' />
          </SidebarHeader>
          <SidebarSeparator />
          <SidebarContent>
            <SidebarGroup>
              <SidebarGroupLabel>Workspace</SidebarGroupLabel>
              <SidebarGroupAction aria-label='Create'>
                <PlusIcon />
              </SidebarGroupAction>
              <SidebarGroupContent>
                <SidebarMenu>
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
            <div className='border rounded-md p-2 text-xs/relaxed'>
              <p className='font-medium'>Pro workspace</p>
              <p className='text-muted-foreground'>42 sessions this week</p>
            </div>
          </SidebarFooter>
          <SidebarRail />
        </Sidebar>
        <SidebarInset className='p-4 gap-3'>
          <div className='flex items-center justify-between'>
            <SidebarTrigger />
            <Badge variant='outline'>session_0f9a</Badge>
          </div>
          <div className='border rounded-lg p-4 text-xs/relaxed space-y-1'>
            <p className='font-medium'>Session transcript</p>
            <p className='text-muted-foreground'>
              Provider connected and waiting for the next user message.
            </p>
          </div>
        </SidebarInset>
      </SidebarProvider>
    </div>
  )
}

export const Expanded: Story = {
  render: () => <SidebarDemo />,
}

export const CollapsedIconMode: Story = {
  render: () => <SidebarDemo defaultOpen={false} />,
}
