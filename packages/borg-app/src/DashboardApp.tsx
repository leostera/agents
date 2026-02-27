import React from 'react'
import { useEffect, useMemo, useState } from 'react'
import { Button, SidebarInset, SidebarProvider } from '@borg/ui'
import { Bell, LayoutDashboard, Plus, Route, ScanSearch, Settings2 } from 'lucide-react'
import { createI18n } from '@borg/i18n'
import { AppSidebar } from './AppSidebar'
import { CommandK, type CommandSectionGroup } from './CommandK'
import { MemoryExplorerPage } from './pages/memory/explorer'
import { ObservabilityOverviewPage } from './pages/observability'
import { ObservabilityAlertsPage } from './pages/observability/alerts'
import { ObservabilityTracingPage } from './pages/observability/tracing'
import { OverviewPage } from './pages/overview'
import { ProvidersPage } from './pages/settings/providers'

type DashboardRouteItem = CommandSectionGroup['items'][number] & {
  path: string
}

type DashboardRouteGroup = {
  id: string
  title: string
  items: DashboardRouteItem[]
}

const SECTION_GROUPS: DashboardRouteGroup[] = [
  {
    id: 'overview',
    title: 'Overview',
    items: [
      {
        id: 'overview-home',
        title: 'Overview',
        icon: LayoutDashboard,
        path: '/',
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
        path: '/observability',
      },
      {
        id: 'observability-alerts',
        title: 'Alerts',
        icon: Bell,
        path: '/observability/alerts',
      },
      {
        id: 'observability-tracing',
        title: 'Tracing',
        icon: Route,
        path: '/observability/tracing',
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
        path: '/memory/explorer',
      },
    ],
  },
  {
    id: 'system-settings',
    title: 'System Settings',
    items: [
      {
        id: 'settings-providers',
        title: 'Providers',
        icon: Settings2,
        path: '/settings/providers',
      },
    ],
  },
]

const ALL_SECTIONS = SECTION_GROUPS.flatMap((group) => group.items)
const SECTION_BY_ID = Object.fromEntries(ALL_SECTIONS.map((section) => [section.id, section])) as Record<string, DashboardRouteItem>
const SECTION_BY_PATH = Object.fromEntries(ALL_SECTIONS.map((section) => [section.path, section])) as Record<string, DashboardRouteItem>
const SECTION_BY_PATH_ALIASES: Record<string, DashboardRouteItem> = {
  '/dashboard': SECTION_BY_ID['overview-home'],
  '/dashbaord': SECTION_BY_ID['overview-home'],
}

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
  const [activeId, setActiveId] = useState<string>(() => {
    if (typeof window === 'undefined') return ALL_SECTIONS[0].id
    const pathname = window.location.pathname.replace(/\/+$/, '') || '/'
    return SECTION_BY_PATH[pathname]?.id ?? SECTION_BY_PATH_ALIASES[pathname]?.id ?? ALL_SECTIONS[0].id
  })
  const [isCommandMenuOpen, setIsCommandMenuOpen] = useState(false)
  const i18n = useMemo(() => createI18n('en'), [])
  const username = useMemo(resolveUsername, [])
  const initials = useMemo(() => initialsFromUsername(username), [username])

  const ActiveSection = useMemo(() => {
    const sectionById: Record<string, () => React.JSX.Element> = {
      'overview-home': () => <OverviewPage />,
      'observability-overview': () => <ObservabilityOverviewPage />,
      'observability-alerts': () => <ObservabilityAlertsPage />,
      'observability-tracing': () => <ObservabilityTracingPage />,
      'memory-explorer': () => <MemoryExplorerPage />,
      'settings-providers': () => <ProvidersPage />,
    }
    return sectionById[activeId] ?? sectionById['overview-home']
  }, [activeId])

  const activeTitle = useMemo(() => {
    const section = ALL_SECTIONS.find((item) => item.id === activeId)
    return section?.title ?? 'Overview'
  }, [activeId])

  const activeSubtitle = useMemo(() => {
    if (activeId === 'settings-providers') {
      return i18n.t('dashboard.subtitle.settings.providers')
    }
    return i18n.t('dashboard.subtitle.default')
  }, [activeId, i18n])

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key.toLowerCase() !== 'k') return
      if (!event.metaKey && !event.ctrlKey) return
      event.preventDefault()
      setIsCommandMenuOpen((open) => !open)
    }

    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [])

  useEffect(() => {
    const handlePopState = () => {
      const pathname = window.location.pathname.replace(/\/+$/, '') || '/'
      const section = SECTION_BY_PATH[pathname] ?? SECTION_BY_PATH_ALIASES[pathname] ?? SECTION_BY_ID['overview-home']
      setActiveId(section.id)
    }

    window.addEventListener('popstate', handlePopState)
    return () => window.removeEventListener('popstate', handlePopState)
  }, [])

  const handleSelectSection = (sectionId: string) => {
    const section = SECTION_BY_ID[sectionId]
    setActiveId(sectionId)
    if (section && window.location.pathname !== section.path) {
      window.history.pushState(null, '', section.path)
    }
    setIsCommandMenuOpen(false)
  }

  const headlineActions = useMemo(() => {
    if (activeId === 'settings-providers') {
      return (
        <Button
          variant='outline'
          onClick={() => window.dispatchEvent(new CustomEvent('providers:open-connect'))}
        >
          <Plus className='size-4' />
          Connect Provider
        </Button>
      )
    }
    return null
  }, [activeId])

  return (
    <section className='borg-dashboard-shell text-foreground'>
      <SidebarProvider defaultOpen>
        <AppSidebar
          activeId={activeId}
          onSelect={handleSelectSection}
          onOpenCommandMenu={() => setIsCommandMenuOpen(true)}
          groups={SECTION_GROUPS}
          username={username}
          initials={initials}
        />
        <SidebarInset className='p-4 md:p-6'>
          <div className='borg-dashboard-content'>
            <div className='borg-dashboard-headline'>
              <div className='borg-dashboard-headline-main'>
                <h2>{activeTitle}</h2>
                <p>{activeSubtitle}</p>
              </div>
              <div className='borg-dashboard-headline-actions'>{headlineActions}</div>
            </div>
          <section id={activeId}>
            <ActiveSection />
          </section>
          </div>
        </SidebarInset>
      </SidebarProvider>
      <CommandK
        open={isCommandMenuOpen}
        onOpenChange={setIsCommandMenuOpen}
        groups={SECTION_GROUPS}
        onSelectSection={handleSelectSection}
      />
    </section>
  )
}
