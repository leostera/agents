import React from 'react'
import {
  Avatar,
  AvatarFallback,
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
import { Search } from 'lucide-react'

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
  subtitle?: string
  activeId: string
  onSelect: (id: string) => void
  groups: SectionGroup[]
  username: string
  initials: string
}

export function AppSidebar({
  title = 'Borg',
  subtitle = 'Dashboard',
  activeId,
  onSelect,
  groups,
  username,
  initials,
  ...props
}: AppSidebarProps) {
  const [query, setQuery] = React.useState('')
  const trimmedQuery = query.trim().toLowerCase()
  const searchLabel = query.length > 0 ? `Filter: ${query}` : 'Search sections'

  const visibleGroups = React.useMemo(() => {
    if (trimmedQuery.length === 0) return groups

    return groups
      .map((group) => ({
        ...group,
        items: group.items.filter((item) => item.title.toLowerCase().includes(trimmedQuery)),
      }))
      .filter((group) => group.items.length > 0)
  }, [groups, trimmedQuery])

  const handleSearch = React.useCallback(() => {
    if (typeof window === 'undefined') return
    const result = window.prompt('Search sections', query)
    if (result === null) return
    setQuery(result)
  }, [query])

  return (
    <Sidebar
      collapsible='none'
      variant='sidebar'
      className='flex h-svh min-h-svh flex-col border-r border-border/60'
      {...props}
    >
      <SidebarHeader className='space-y-3 p-3'>
        <div className='flex items-center gap-2 rounded-lg border border-border/60 bg-gradient-to-br from-card to-muted/30 p-3'>
          <div className='min-w-0 flex-1'>
            <p className='truncate text-[10px] uppercase tracking-[0.16em] text-muted-foreground'>{title}</p>
            <h1 className='truncate text-base font-semibold'>{subtitle}</h1>
          </div>
          <button
            type='button'
            onClick={handleSearch}
            className='text-muted-foreground hover:text-foreground inline-flex size-8 shrink-0 items-center justify-center rounded-md border border-border/60 bg-background/75 transition-colors'
            aria-label={searchLabel}
            title={searchLabel}
          >
            <Search className='size-4' />
          </button>
        </div>
      </SidebarHeader>
      <SidebarSeparator className='mx-3' />
      <SidebarContent className='flex-1 px-1 pb-2'>
        {visibleGroups.map((group) => (
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
        {visibleGroups.length === 0 ? (
          <SidebarGroupContent>
            <p className='px-2 py-1 text-xs text-muted-foreground'>No sections match your search.</p>
          </SidebarGroupContent>
        ) : null}
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
