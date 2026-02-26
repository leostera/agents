import React from 'react'
import { useEffect, useMemo, useState } from 'react'

import { createI18n } from '@borg/i18n'
import { Button, Card, UiSelect } from '@borg/ui'
import './styles.css'

type DashboardAppProps = {
  className?: string
}

type JsonMap = Record<string, unknown>

function toArray(value: unknown): Array<JsonMap> {
  return Array.isArray(value) ? (value as Array<JsonMap>) : []
}

async function getJson(path: string): Promise<JsonMap> {
  const response = await fetch(path, { headers: { accept: 'application/json' } })
  if (!response.ok) {
    throw new Error(`${path} failed with ${response.status}`)
  }
  return (await response.json()) as JsonMap
}

export function DashboardApp(props: DashboardAppProps) {
  const i18n = useMemo(() => createI18n('en'), [])
  const [port, setPort] = useState('telegram')
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')
  const [users, setUsers] = useState<Array<JsonMap>>([])
  const [sessions, setSessions] = useState<Array<JsonMap>>([])
  const [providers, setProviders] = useState<Array<JsonMap>>([])
  const [agents, setAgents] = useState<Array<JsonMap>>([])
  const [policies, setPolicies] = useState<Array<JsonMap>>([])
  const [portSettings, setPortSettings] = useState<Array<JsonMap>>([])
  const [portBindings, setPortBindings] = useState<Array<JsonMap>>([])

  async function loadAll(selectedPort: string) {
    setLoading(true)
    setError('')
    try {
      const [usersData, sessionsData, providersData, agentsData, policiesData, settingsData, bindingsData] =
        await Promise.all([
          getJson('/api/users?limit=50'),
          getJson('/api/sessions?limit=50'),
          getJson('/api/providers?limit=20'),
          getJson('/api/agents/specs?limit=20'),
          getJson('/api/policies?limit=50'),
          getJson(`/api/ports/${encodeURIComponent(selectedPort)}/settings?limit=50`),
          getJson(`/api/ports/${encodeURIComponent(selectedPort)}/bindings?limit=50`),
        ])
      setUsers(toArray(usersData.users))
      setSessions(toArray(sessionsData.sessions))
      setProviders(toArray(providersData.providers))
      setAgents(toArray(agentsData.agent_specs))
      setPolicies(toArray(policiesData.policies))
      setPortSettings(toArray(settingsData.settings))
      setPortBindings(toArray(bindingsData.bindings))
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : 'Failed to load dashboard data'
      setError(message)
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    loadAll(port)
  }, [port])

  function renderRows(items: Array<JsonMap>, fields: string[]) {
    if (!items.length) {
      return <p className='dash-empty'>No rows</p>
    }
    return (
      <div className='dash-table-wrap'>
        <table className='dash-table'>
          <thead>
            <tr>
              {fields.map((field) => (
                <th key={field}>{field}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {items.slice(0, 20).map((item, idx) => (
              <tr key={`${idx}-${String(item[fields[0]])}`}>
                {fields.map((field) => (
                  <td key={field}>{String(item[field] ?? '')}</td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    )
  }

  return (
    <section className={props.className ?? 'dash-root'}>
      <header className='dash-top'>
        <div>
          <p className='step'>{i18n.t('dashboard.step')}</p>
          <h2 className='onboard-heading'>{i18n.t('dashboard.title')}</h2>
          <p className='onboard-tagline'>{i18n.t('dashboard.tagline')}</p>
        </div>
        <div className='dash-actions'>
          <UiSelect
            value={port}
            onValueChange={setPort}
            options={[
              { label: 'telegram', value: 'telegram' },
              { label: 'http', value: 'http' },
            ]}
          />
          <Button tone='subtle' onClick={() => loadAll(port)} disabled={loading}>
            {loading ? 'Loading...' : 'Refresh'}
          </Button>
        </div>
      </header>

      {error ? <p className='notice-error'>{error}</p> : null}

      <section className='dash-metrics'>
        <div className='dash-metric'><span>Users</span><strong>{users.length}</strong></div>
        <div className='dash-metric'><span>Sessions</span><strong>{sessions.length}</strong></div>
        <div className='dash-metric'><span>Providers</span><strong>{providers.length}</strong></div>
        <div className='dash-metric'><span>Agents</span><strong>{agents.length}</strong></div>
        <div className='dash-metric'><span>Policies</span><strong>{policies.length}</strong></div>
        <div className='dash-metric'><span>Port</span><strong>{port}</strong></div>
      </section>

      <section className='dash-grid'>
        <Card title='Users'>{renderRows(users, ['user_key', 'updated_at'])}</Card>
        <Card title='Sessions'>{renderRows(sessions, ['session_id', 'user_key', 'port'])}</Card>
        <Card title='Providers'>{renderRows(providers, ['provider', 'updated_at'])}</Card>
        <Card title='Agent Specs'>{renderRows(agents, ['agent_id', 'model'])}</Card>
        <Card title='Policies'>{renderRows(policies, ['policy_id', 'updated_at'])}</Card>
        <Card title='Port Settings'>{renderRows(portSettings, ['key', 'value'])}</Card>
        <Card title='Port Bindings'>{renderRows(portBindings, ['conversation_key', 'session_id', 'agent_id'])}</Card>
      </section>
    </section>
  )
}
