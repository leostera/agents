import React from 'react'
import { useMemo, useState } from 'react'
import { SidebarInset, SidebarProvider } from '@borg/ui'
import { Bell, LayoutDashboard, Route, ScanSearch } from 'lucide-react'
import { MemoryExplorer } from '@borg/explorer'
import { AppSidebar } from './AppSidebar'
import { AlertsSection } from './sections/observability/AlertsSection'
import { TracingSection } from './sections/observability/TracingSection'
import { OverviewSection } from './sections/overview/OverviewSection'
import { UserOverviewSection } from './sections/overview/UserOverviewSection'

type DashboardSection = {
  id: string
  title: string
  icon: React.ComponentType<{ className?: string }>
}

type SectionGroup = {
  id: string
  title: string
  items: DashboardSection[]
}

const SECTION_GROUPS: SectionGroup[] = [
  {
    id: 'overview',
    title: 'Overview',
    items: [
      {
        id: 'overview-home',
        title: 'Overview',
        icon: LayoutDashboard,
      },
    ],
  },
  {
    id: 'observability',
    title: 'Observability',
    items: [
      {
        id: 'observability-overview',
        title: 'Overview',
        icon: LayoutDashboard,
      },
      {
        id: 'observability-alerts',
        title: 'Alerts',
        icon: Bell,
      },
      {
        id: 'observability-tracing',
        title: 'Tracing',
        icon: Route,
      },
    ],
  },
  {
    id: 'memory',
    title: 'Memory',
    items: [
      {
        id: 'memory-explorer',
        title: 'Explorer',
        icon: ScanSearch,
      },
    ],
  },
]

const ALL_SECTIONS = SECTION_GROUPS.flatMap((group) => group.items)

function resolveUsername(): string {
  if (typeof window === 'undefined') return 'friend'
  const fromQuery = new URLSearchParams(window.location.search).get('user')?.trim()
  return fromQuery && fromQuery.length > 0 ? fromQuery : 'friend'
}

function initialsFromUsername(username: string): string {
  return (
    username
      .split(/\s+/)
      .filter(Boolean)
      .slice(0, 2)
      .map((part) => part[0]?.toUpperCase() ?? '')
      .join('') || 'U'
  )
}

export function DashboardApp() {
  const [activeId, setActiveId] = useState<string>(ALL_SECTIONS[0].id)
  const username = useMemo(resolveUsername, [])
  const initials = useMemo(() => initialsFromUsername(username), [username])

  const ActiveSection = useMemo(() => {
    const sectionById: Record<string, () => React.JSX.Element> = {
      'overview-home': () => <UserOverviewSection />,
      'observability-overview': () => <OverviewSection />,
      'observability-alerts': () => <AlertsSection />,
      'observability-tracing': () => <TracingSection />,
      'memory-explorer': () => (
        <section className='space-y-3'>
          <h2 className='text-lg font-semibold'>Memory Explorer</h2>
          <MemoryExplorer />
        </section>
      ),
    }
    return sectionById[activeId] ?? sectionById['overview-home']
  }, [activeId])

  const activeTitle = useMemo(() => {
    const section = ALL_SECTIONS.find((item) => item.id === activeId)
    return section?.title ?? 'Overview'
  }, [activeId])

  return (
    <section className='borg-dashboard-shell text-foreground'>
      <SidebarProvider defaultOpen>
        <AppSidebar
          activeId={activeId}
          onSelect={setActiveId}
          groups={SECTION_GROUPS}
          username={username}
          initials={initials}
        />
        <SidebarInset className='p-4 md:p-6'>
          <div className='borg-dashboard-content'>
            <div className='borg-dashboard-headline'>
              <h2>{activeTitle}</h2>
              <p>Platform and session intelligence</p>
            </div>
          <section id={activeId}>
            <ActiveSection />
          </section>
          </div>
        </SidebarInset>
      </SidebarProvider>
    </section>
  )
}
