import React from 'react'
import { useEffect, useMemo, useState } from 'react'
import { Button, SidebarInset, SidebarProvider } from '@borg/ui'
import { Bell, LayoutDashboard, Map, Plus, Route, ScanSearch, Settings2 } from 'lucide-react'
import { createI18n } from '@borg/i18n'
import { BorgApiError, createBorgApiClient } from '@borg/api'
import { AppSidebar } from './AppSidebar'
import { CommandK, type CommandSectionGroup } from './CommandK'
import { MemoryEntityPage } from './pages/memory/entity'
import { MemoryExplorerPage } from './pages/memory/explorer'
import { MemoryGraphPage } from './pages/memory/graph'
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

type ResolvedDashboardRoute = {
  id: string
  entityUri: string | null
  explorerUri: string | null
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
      {
        id: 'memory-graph',
        title: 'Graph',
        icon: Map,
        path: '/memory/graph',
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
  '/memory': SECTION_BY_ID['memory-graph'],
  '/memory/search': SECTION_BY_ID['memory-explorer'],
}
const MEMORY_ENTITY_PREFIX = '/memory/entity/'
const MEMORY_EXPLORER_PREFIX = '/memory/explorer/'
const borgApi = createBorgApiClient()

function normalizePathname(pathname: string): string {
  return pathname.replace(/\/+$/, '') || '/'
}

function resolveRouteFromPath(pathname: string): ResolvedDashboardRoute {
  const normalizedPathname = normalizePathname(pathname)
  if (normalizedPathname.startsWith(MEMORY_ENTITY_PREFIX) && normalizedPathname.length > MEMORY_ENTITY_PREFIX.length) {
    const encodedUri = normalizedPathname.slice(MEMORY_ENTITY_PREFIX.length)
    try {
      return { id: 'memory-entity', entityUri: decodeURIComponent(encodedUri), explorerUri: null }
    } catch {
      return { id: 'memory-entity', entityUri: encodedUri, explorerUri: null }
    }
  }
  if (normalizedPathname.startsWith(MEMORY_EXPLORER_PREFIX) && normalizedPathname.length > MEMORY_EXPLORER_PREFIX.length) {
    const encodedUri = normalizedPathname.slice(MEMORY_EXPLORER_PREFIX.length)
    try {
      return { id: 'memory-explorer', entityUri: null, explorerUri: decodeURIComponent(encodedUri) }
    } catch {
      return { id: 'memory-explorer', entityUri: null, explorerUri: encodedUri }
    }
  }

  const section =
    SECTION_BY_PATH[normalizedPathname] ??
    SECTION_BY_PATH_ALIASES[normalizedPathname] ??
    SECTION_BY_ID['overview-home']
  return { id: section.id, entityUri: null, explorerUri: null }
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
  const [route, setRoute] = useState<ResolvedDashboardRoute>(() => {
    if (typeof window === 'undefined') return { id: ALL_SECTIONS[0].id, entityUri: null, explorerUri: null }
    return resolveRouteFromPath(window.location.pathname)
  })
  const activeId = route.id
  const [isCommandMenuOpen, setIsCommandMenuOpen] = useState(false)
  const [isOffline, setIsOffline] = useState(false)
  const i18n = useMemo(() => createI18n('en'), [])
  const username = useMemo(resolveUsername, [])
  const initials = useMemo(() => initialsFromUsername(username), [username])

  const ActiveSection = useMemo(() => {
    const sectionById: Record<string, () => React.JSX.Element> = {
      'overview-home': () => <OverviewPage />,
      'observability-overview': () => <ObservabilityOverviewPage />,
      'observability-alerts': () => <ObservabilityAlertsPage />,
      'observability-tracing': () => <ObservabilityTracingPage />,
      'memory-graph': () => <MemoryGraphPage />,
      'memory-explorer': () => <MemoryExplorerPage explorerUri={route.explorerUri ?? undefined} />,
      'memory-entity': () => <MemoryEntityPage entityUri={route.entityUri ?? ''} />,
      'settings-providers': () => <ProvidersPage />,
    }
    return sectionById[activeId] ?? sectionById['overview-home']
  }, [activeId, route.entityUri, route.explorerUri])

  const activeTitle = useMemo(() => {
    if (activeId === 'memory-entity') return 'Entity'
    const section = ALL_SECTIONS.find((item) => item.id === activeId)
    return section?.title ?? 'Overview'
  }, [activeId])

  const activeSubtitle = useMemo(() => {
    if (activeId === 'settings-providers') {
      return i18n.t('dashboard.subtitle.settings.providers')
    }
    if (activeId === 'memory-entity') {
      return route.entityUri ?? i18n.t('dashboard.subtitle.default')
    }
    return i18n.t('dashboard.subtitle.default')
  }, [activeId, i18n, route.entityUri])

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
      setRoute(resolveRouteFromPath(window.location.pathname))
    }

    window.addEventListener('popstate', handlePopState)
    return () => window.removeEventListener('popstate', handlePopState)
  }, [])

  useEffect(() => {
    let isActive = true
    let timeoutId: number | undefined

    const checkConnectivity = async () => {
      try {
        await borgApi.listProviders(1)
        if (isActive) {
          setIsOffline(false)
        }
      } catch (error) {
        if (!isActive) return
        if (error instanceof BorgApiError && typeof error.status === 'number') {
          setIsOffline(false)
        } else {
          setIsOffline(true)
        }
      } finally {
        if (isActive) {
          timeoutId = window.setTimeout(() => {
            void checkConnectivity()
          }, 15000)
        }
      }
    }

    void checkConnectivity()
    return () => {
      isActive = false
      if (timeoutId) window.clearTimeout(timeoutId)
    }
  }, [])

  const handleSelectSection = (sectionId: string) => {
    const section = SECTION_BY_ID[sectionId]
    setRoute({ id: sectionId, entityUri: null, explorerUri: null })
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

  const isExplorerImmersive = activeId === 'memory-graph'
  const isHeadlineHidden = activeId === 'memory-graph' || activeId === 'memory-explorer'

  return (
    <section className='borg-dashboard-shell text-foreground'>
      <SidebarProvider defaultOpen>
        <AppSidebar
          activeId={activeId}
          isOffline={isOffline}
          onSelect={handleSelectSection}
          onOpenCommandMenu={() => setIsCommandMenuOpen(true)}
          groups={SECTION_GROUPS}
          username={username}
          initials={initials}
        />
        <SidebarInset className={isExplorerImmersive ? 'p-0 md:p-0' : 'p-4 md:p-6'}>
          <div className={`borg-dashboard-content${isExplorerImmersive ? ' borg-dashboard-content--full' : ''}`}>
            {!isHeadlineHidden ? (
              <div className='borg-dashboard-headline'>
                <div className='borg-dashboard-headline-main'>
                  <h2>{activeTitle}</h2>
                  <p>{activeSubtitle}</p>
                </div>
                <div className='borg-dashboard-headline-actions'>{headlineActions}</div>
              </div>
            ) : null}
            <section id={activeId} className={isExplorerImmersive ? 'borg-dashboard-section--full' : undefined}>
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
