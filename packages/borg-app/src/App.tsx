import React from 'react'
import { useMemo } from 'react'

import { DashboardApp } from '@borg/dashboard'
import { createI18n } from '@borg/i18n'
import { OnboardApp } from '@borg/onboard'

const ONBOARD_PATH = '/onboard'
const DASHBOARD_PATH = '/dashboard'

export function App() {
  const i18n = useMemo(() => createI18n('en'), [])
  const pathname = window.location.pathname

  if (pathname === ONBOARD_PATH || pathname === '/') {
    return <OnboardApp />
  }

  if (pathname === DASHBOARD_PATH) {
    return <DashboardApp />
  }

  return (
    <section className='card'>
      <p className='notice-error'>
        {i18n.t('web.unknown_route')}: {pathname}
      </p>
    </section>
  )
}
