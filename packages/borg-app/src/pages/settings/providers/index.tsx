import React from 'react'
import { BorgApiError, createBorgApiClient } from '@borg/api'
import {
  Badge,
  Button,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@borg/ui'
import { CheckCircle2, LoaderCircle, Pause, Play, TriangleAlert, Unplug } from 'lucide-react'
import { ConnectProviderForm } from './ConnectProviderForm'

type ProviderRecord = {
  provider: string
  api_key: string
  created_at: string
  updated_at: string
}

type ProviderMetrics = {
  providerLabel: string
  tokensUsed: string
  tokenRate: string
  models: string[]
  cost: string
  lastUsedOn: string
  lastSession: string
}

const SUPPORTED_PROVIDERS = ['openai', 'openrouter'] as const
const borgApi = createBorgApiClient()

const PROVIDER_METRICS: Record<(typeof SUPPORTED_PROVIDERS)[number], ProviderMetrics> = {
  openai: {
    providerLabel: 'chatgpt',
    tokensUsed: '100000',
    tokenRate: '100 / hour',
    models: ['gpt-5.3-codex', 'gpt-4o-mini'],
    cost: '$33',
    lastUsedOn: '33 seconds ago',
    lastSession: 'borg:session:dashboard_9f2d',
  },
  openrouter: {
    providerLabel: 'openrouter',
    tokensUsed: '64000',
    tokenRate: '42 / hour',
    models: ['openai/gpt-4o-mini', 'meta-llama/3.3-70b-instruct'],
    cost: '$19.40',
    lastUsedOn: '5 minutes ago',
    lastSession: 'borg:session:ops_14d2',
  },
}

function formatProviderName(provider: string): string {
  if (provider === 'openai') return 'OpenAI'
  if (provider === 'openrouter') return 'OpenRouter'
  return provider
}

export function ProvidersPage() {
  const [providersByName, setProvidersByName] = React.useState<Record<string, ProviderRecord>>({})
  const [pausedProviders, setPausedProviders] = React.useState<Record<string, boolean>>({})
  const [isLoading, setIsLoading] = React.useState(true)
  const [isDialogOpen, setIsDialogOpen] = React.useState(false)
  const [openRouterApiKey, setOpenRouterApiKey] = React.useState('')
  const [isSavingOpenRouter, setIsSavingOpenRouter] = React.useState(false)
  const [isStartingOpenAi, setIsStartingOpenAi] = React.useState(false)
  const [statusMessage, setStatusMessage] = React.useState<string | null>(null)
  const [errorMessage, setErrorMessage] = React.useState<string | null>(null)

  const loadProviders = React.useCallback(async () => {
    setIsLoading(true)
    setErrorMessage(null)
    try {
      const providers = await borgApi.listProviders(100)
      const byName = Object.fromEntries(providers.map((provider) => [provider.provider, provider]))
      setProvidersByName(byName)
    } catch (error) {
      setErrorMessage(error instanceof Error ? error.message : 'Unable to load providers')
    } finally {
      setIsLoading(false)
    }
  }, [])

  React.useEffect(() => {
    void loadProviders()
  }, [loadProviders])

  React.useEffect(() => {
    const handleOpenConnect = () => setIsDialogOpen(true)
    window.addEventListener('providers:open-connect', handleOpenConnect)
    return () => window.removeEventListener('providers:open-connect', handleOpenConnect)
  }, [])

  const handleSaveOpenRouter = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    const apiKey = openRouterApiKey.trim()
    if (apiKey.length === 0) {
      setErrorMessage('OpenRouter API key is required')
      return
    }

    setIsSavingOpenRouter(true)
    setErrorMessage(null)
    setStatusMessage(null)
    try {
      await borgApi.upsertProviderApiKey('openrouter', apiKey)
      setOpenRouterApiKey('')
      setStatusMessage('OpenRouter API key saved')
      await loadProviders()
    } catch (error) {
      setErrorMessage(error instanceof Error ? error.message : 'Unable to save OpenRouter key')
    } finally {
      setIsSavingOpenRouter(false)
    }
  }

  const handleStartOpenAiSignIn = async () => {
    setIsStartingOpenAi(true)
    setErrorMessage(null)
    setStatusMessage(null)
    try {
      await borgApi.startOpenAiDeviceCode()
      setStatusMessage('OpenAI device-code sign-in started. Continue in your Codex auth flow.')
      await loadProviders()
    } catch (error) {
      if (error instanceof BorgApiError && error.status === 404) {
        setErrorMessage('OpenAI device-code flow is not wired in the API yet')
      } else {
        setErrorMessage(error instanceof Error ? error.message : 'Unable to start OpenAI sign-in')
      }
    } finally {
      setIsStartingOpenAi(false)
    }
  }

  const handleDisconnect = async (provider: string) => {
    setErrorMessage(null)
    setStatusMessage(null)
    try {
      await borgApi.deleteProvider(provider, { ignoreNotFound: true })
      setStatusMessage(`${formatProviderName(provider)} disconnected`)
      await loadProviders()
    } catch (error) {
      setErrorMessage(error instanceof Error ? error.message : `Unable to disconnect ${formatProviderName(provider)}`)
    }
  }

  const handleTogglePause = (provider: string) => {
    setPausedProviders((current) => ({ ...current, [provider]: !current[provider] }))
  }

  return (
    <section className='space-y-4'>
      {errorMessage ? (
        <div className='flex items-center gap-2 rounded-md border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs text-destructive'>
          <TriangleAlert className='size-3.5' />
          {errorMessage}
        </div>
      ) : null}
      {statusMessage ? (
        <div className='flex items-center gap-2 rounded-md border border-emerald-600/30 bg-emerald-600/10 px-3 py-2 text-xs text-emerald-700'>
          <CheckCircle2 className='size-3.5' />
          {statusMessage}
        </div>
      ) : null}

      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>Provider</TableHead>
            <TableHead>Tokens Used</TableHead>
            <TableHead>Token Rate</TableHead>
            <TableHead>Models</TableHead>
            <TableHead>Cost</TableHead>
            <TableHead>Last Used On</TableHead>
            <TableHead>Last Session</TableHead>
            <TableHead>Actions</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {SUPPORTED_PROVIDERS.map((name) => {
            const provider = providersByName[name]
            const isConnected = Boolean(provider?.api_key)
            const metrics = PROVIDER_METRICS[name]
            const isPaused = Boolean(pausedProviders[name])

            return (
              <TableRow key={name}>
                <TableCell className='font-medium'>
                  <div className='flex items-center gap-2'>
                    <span>{metrics.providerLabel}</span>
                    {!isConnected ? <Badge variant='outline'>not connected</Badge> : null}
                    {isPaused ? <Badge variant='outline'>paused</Badge> : null}
                  </div>
                </TableCell>
                <TableCell>{isConnected ? metrics.tokensUsed : '—'}</TableCell>
                <TableCell>{isConnected ? metrics.tokenRate : '—'}</TableCell>
                <TableCell className='max-w-[280px]'>
                  <div className='flex flex-wrap gap-1'>
                    {isConnected
                      ? metrics.models.map((model) => (
                          <Badge key={model} variant='secondary'>
                            {model}
                          </Badge>
                        ))
                      : '—'}
                  </div>
                </TableCell>
                <TableCell>{isConnected ? metrics.cost : '—'}</TableCell>
                <TableCell>{isConnected ? metrics.lastUsedOn : '—'}</TableCell>
                <TableCell className='font-mono text-[11px]'>{isConnected ? metrics.lastSession : '—'}</TableCell>
                <TableCell>
                  <div className='flex items-center gap-1'>
                    <Button
                      size='icon-sm'
                      variant='outline'
                      onClick={() => void handleDisconnect(name)}
                      disabled={!isConnected}
                      aria-label={`Disconnect ${metrics.providerLabel}`}
                      title='Disconnect'
                    >
                      <Unplug className='size-3.5' />
                    </Button>
                    <Button
                      size='icon-sm'
                      variant='outline'
                      onClick={() => handleTogglePause(name)}
                      disabled={!isConnected}
                      aria-label={`${isPaused ? 'Resume' : 'Pause'} ${metrics.providerLabel}`}
                      title={isPaused ? 'Resume' : 'Pause'}
                    >
                      {isPaused ? <Play className='size-3.5' /> : <Pause className='size-3.5' />}
                    </Button>
                  </div>
                </TableCell>
              </TableRow>
            )
          })}
          {!isLoading && Object.keys(providersByName).length === 0 ? (
            <TableRow>
              <TableCell colSpan={8} className='text-muted-foreground'>
                No providers configured yet.
              </TableCell>
            </TableRow>
          ) : null}
          {isLoading ? (
            <TableRow>
              <TableCell colSpan={8}>
                <span className='text-muted-foreground inline-flex items-center gap-2'>
                  <LoaderCircle className='size-3.5 animate-spin' />
                  Loading providers...
                </span>
              </TableCell>
            </TableRow>
          ) : null}
        </TableBody>
      </Table>

      <ConnectProviderForm
        open={isDialogOpen}
        onOpenChange={setIsDialogOpen}
        isStartingOpenAi={isStartingOpenAi}
        isSavingOpenRouter={isSavingOpenRouter}
        openRouterApiKey={openRouterApiKey}
        onOpenRouterApiKeyChange={setOpenRouterApiKey}
        onStartOpenAiSignIn={() => void handleStartOpenAiSignIn()}
        onSaveOpenRouter={handleSaveOpenRouter}
      />
    </section>
  )
}
