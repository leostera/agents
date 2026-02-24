import React from 'react'
import { useMemo } from 'react'

import { createI18n } from '@borg/i18n'

type DashboardAppProps = {
  className?: string
}

export function DashboardApp(props: DashboardAppProps) {
  const i18n = useMemo(() => createI18n('en'), [])

  return (
    <section className={props.className ?? 'card'}>
      <p className='step'>{i18n.t('dashboard.step')}</p>
      <h2 className='onboard-heading'>{i18n.t('dashboard.title')}</h2>
      <p className='onboard-tagline'>{i18n.t('dashboard.tagline')}</p>
    </section>
  )
}
