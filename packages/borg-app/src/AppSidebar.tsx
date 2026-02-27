import React from 'react'
import {
  Avatar,
  AvatarFallback,
  Badge,
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarRail,
  SidebarSeparator,
} from '@borg/ui'
import { Command } from 'lucide-react'

type SectionItem = {
  id: string
  title: string
  icon: React.ComponentType<{ className?: string }>
}

type SectionGroup = {
  id: string
  title: string
  items: SectionItem[]
}

type AppSidebarProps = React.ComponentProps<typeof Sidebar> & {
  title?: string
  activeId: string
  isOffline?: boolean
  onSelect: (id: string) => void
  onOpenCommandMenu?: () => void
  groups: SectionGroup[]
  username: string
  initials: string
}

export function AppSidebar({
  title = 'Borg',
  activeId,
  isOffline = false,
  onSelect,
  onOpenCommandMenu,
  groups,
  username,
  initials,
  ...props
}: AppSidebarProps) {
  const handleSearch = React.useCallback(() => {
    onOpenCommandMenu?.()
  }, [onOpenCommandMenu])

  return (
    <Sidebar
      collapsible='none'
      variant='sidebar'
      className='flex h-svh min-h-svh flex-col border-r border-border/60'
      {...props}
    >
      <SidebarHeader className='p-3'>
        <div className='px-1'>
          <div className='flex items-center gap-2'>
            <p className='min-w-0 truncate text-[10px] uppercase tracking-[0.16em] text-muted-foreground'>{title}</p>
            {isOffline ? (
              <Badge
                variant='outline'
                className='border-red-500/40 bg-red-500/10 px-1.5 py-0 text-[9px] font-medium uppercase tracking-[0.08em] text-red-700'
              >
                Offline
              </Badge>
            ) : null}
            <div className='flex-1' />
            <button
              type='button'
              onClick={handleSearch}
              className='text-muted-foreground hover:text-foreground inline-flex h-7 shrink-0 items-center gap-1.5 rounded-md border border-border/60 px-2 font-mono text-[10px] transition-colors'
              aria-label='Open command menu (Cmd+K)'
              title='Open command menu (Cmd+K)'
            >
              <Command className='size-3.5' />
              <span>K</span>
            </button>
          </div>
        </div>
      </SidebarHeader>
      <SidebarSeparator className='mx-3' />
      <SidebarContent className='flex-1 px-1 pb-2'>
        {groups.map((group) => (
          <SidebarGroup key={group.id} className='py-2'>
            <SidebarGroupLabel className='px-2 text-[10px] uppercase tracking-[0.14em] text-muted-foreground'>
              {group.title}
            </SidebarGroupLabel>
            <SidebarGroupContent>
              <SidebarMenu className='flex flex-col'>
                {group.items.map((section) => {
                  const Icon = section.icon

                  return (
                    <SidebarMenuItem key={section.id}>
                      <SidebarMenuButton
                        isActive={activeId === section.id}
                        onClick={() => onSelect(section.id)}
                        className='h-9 justify-start rounded-lg text-[13px] font-medium'
                      >
                        <Icon className='size-4' />
                        <span>{section.title}</span>
                      </SidebarMenuButton>
                    </SidebarMenuItem>
                  )
                })}
              </SidebarMenu>
            </SidebarGroupContent>
          </SidebarGroup>
        ))}
      </SidebarContent>
      <SidebarFooter className='mt-auto p-3'>
        <div className='flex min-w-0 w-full items-center gap-2 overflow-hidden rounded-lg border border-border/60 bg-muted/20 p-2'>
          <Avatar className='size-8 border border-border/60'>
            <AvatarFallback>{initials}</AvatarFallback>
          </Avatar>
          <div className='min-w-0'>
            <p className='truncate text-sm font-medium'>{username}</p>
            <p className='text-xs text-muted-foreground'>Workspace owner</p>
          </div>
        </div>
      </SidebarFooter>
      <SidebarRail />
    </Sidebar>
  )
}
