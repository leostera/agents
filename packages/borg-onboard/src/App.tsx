import React from 'react'
import { useEffect, useMemo, useState } from 'react'
import { Effect, pipe } from 'effect'

import { createI18n } from '@borg/i18n'
import { Session, type SessionMessage } from '@borg/ui'

const OPENAI_PROVIDER = 'openai'
const OPENROUTER_PROVIDER = 'openrouter'
const PROVIDER_MESSAGE_ID = 'u-provider'
const PROVIDER_API_KEY_MESSAGE_ID = 'i-provider-key'
const CONNECT_ACTION_ID = 'connect'
const PROVIDER_RESPONSE_DELAY_MS = 350

type ProviderOption = {
  labelKey: 'onboard.provider.openai' | 'onboard.provider.openrouter'
  value: string
  icon: 'openai' | 'openrouter'
}

const PROVIDER_OPTIONS: Array<ProviderOption> = [
  { labelKey: 'onboard.provider.openai', value: OPENAI_PROVIDER, icon: 'openai' },
  { labelKey: 'onboard.provider.openrouter', value: OPENROUTER_PROVIDER, icon: 'openrouter' },
]

type SessionState = {
  choices: Record<string, string>
  saving: boolean
  error: string
  saved: boolean
}

function getUsername() {
  const fromQuery = new URLSearchParams(window.location.search).get('user')
  if (fromQuery && fromQuery.trim().length > 0) return fromQuery.trim()
  return 'friend'
}

function isProviderSupported(value: string) {
  return PROVIDER_OPTIONS.some((option) => option.value === value)
}

function saveProvider(provider: string, apiKey: string, saveFailedMessage: string) {
  return pipe(
    Effect.tryPromise(() =>
      fetch(`/api/providers/${encodeURIComponent(provider)}`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ api_key: apiKey }),
      }),
    ),
    Effect.flatMap((resp) => {
      if (!resp.ok) {
        return Effect.fail(new Error(saveFailedMessage))
      }
      return Effect.tryPromise(() => resp.json())
    }),
  )
}

function formatTimestamp(value: Date) {
  return value.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
}

export function OnboardApp() {
  const i18n = useMemo(() => createI18n('en'), [])
  const username = useMemo(getUsername, [])
  const startedAt = useMemo(() => new Date(), [])
  const [state, setState] = useState<SessionState>({
    choices: {},
    saving: false,
    error: '',
    saved: false,
  })
  const [animatedCompleted, setAnimatedCompleted] = useState<Record<string, boolean>>({})
  const [providerResponseReady, setProviderResponseReady] = useState<boolean>(false)

  const selectedProvider = state.choices[PROVIDER_MESSAGE_ID] ?? ''
  const selectedProviderLabel = useMemo(() => {
    const selectedOption = PROVIDER_OPTIONS.find((option) => option.value === selectedProvider)
    return selectedOption ? i18n.t(selectedOption.labelKey) : ''
  }, [i18n, selectedProvider])

  useEffect(() => {
    if (!isProviderSupported(selectedProvider)) {
      setProviderResponseReady(false)
      return
    }

    const timer = window.setTimeout(() => {
      setProviderResponseReady(true)
    }, PROVIDER_RESPONSE_DELAY_MS)

    return () => {
      window.clearTimeout(timer)
    }
  }, [selectedProvider])

  const baseMessages = useMemo<Array<SessionMessage>>(
    () => [
      {
        id: 'm-welcome',
        type: 'message',
        author: 'agent',
        content: i18n.t('onboard.agent.welcome', { username }),
        timestamp: formatTimestamp(startedAt),
      },
      {
        id: PROVIDER_MESSAGE_ID,
        type: 'message',
        author: 'user',
        content: selectedProviderLabel,
        timestamp: formatTimestamp(new Date(startedAt.getTime() + 30_000)),
        choices:
          selectedProvider.length === 0
            ? {
                name: i18n.t('onboard.choice.provider_name'),
                options: PROVIDER_OPTIONS.map((option) => ({
                  label: i18n.t(option.labelKey),
                  value: option.value,
                  icon: option.icon,
                })),
              }
            : undefined,
      },
    ],
    [i18n, selectedProvider, selectedProviderLabel, startedAt, username],
  )

  const messages = useMemo<Array<SessionMessage>>(() => {
    if (!animatedCompleted['m-welcome']) {
      return baseMessages.filter((message) => message.id === 'm-welcome')
    }

    if (!isProviderSupported(selectedProvider) || !providerResponseReady) return baseMessages

    const withInput: Array<SessionMessage> = [
      ...baseMessages,
      {
        id: 'm-key',
        type: 'message',
        author: 'agent',
        content: i18n.t('onboard.agent.provider_key_prompt', { provider: selectedProviderLabel }),
        timestamp: formatTimestamp(new Date(startedAt.getTime() + 60_000)),
      },
      ...(!animatedCompleted['m-key']
        ? []
        : [
            {
              id: 'u-key',
              type: 'message' as const,
              author: 'user' as const,
              content: '',
              timestamp: formatTimestamp(new Date(startedAt.getTime() + 90_000)),
              input: {
                id: PROVIDER_API_KEY_MESSAGE_ID,
                inputType: 'text' as const,
                name: i18n.t('onboard.field.api_key'),
                placeholder: 'sk-...',
                secret: true,
              },
              actions: [
                {
                  id: CONNECT_ACTION_ID,
                  label: state.saving
                    ? i18n.t('onboard.action.saving')
                    : i18n.t('onboard.action.save_api_key'),
                  disabled: state.saving,
                },
              ],
            },
          ]),
    ]

    if (state.error.length > 0) {
      return [
        ...withInput,
        {
          id: 'm-error',
          type: 'message',
          author: 'system',
          content: state.error,
          timestamp: formatTimestamp(new Date(startedAt.getTime() + 90_000)),
        },
      ]
    }

    if (state.saved) {
      return [
        ...withInput,
        {
          id: 'm-saved',
          type: 'message',
          author: 'system',
          content: i18n.t('onboard.notice.saved'),
          timestamp: formatTimestamp(new Date(startedAt.getTime() + 90_000)),
        },
      ]
    }

    return withInput
  }, [
    animatedCompleted,
    baseMessages,
    i18n,
    providerResponseReady,
    selectedProvider,
    selectedProviderLabel,
    startedAt,
    state.error,
    state.saved,
    state.saving,
  ])

  const animatedIds = useMemo(() => {
    const ids: Array<string> = []
    if (!animatedCompleted['m-welcome']) {
      ids.push('m-welcome')
    } else if (isProviderSupported(selectedProvider) && !animatedCompleted['m-key']) {
      ids.push('m-key')
    }
    return ids
  }, [animatedCompleted, selectedProvider])

  return (
    <div className='onboard-chat-shell'>
      <Session
        messages={messages}
        choices={state.choices}
        animatedIds={animatedIds}
        agentName='Borg'
        systemName='Borg'
        onChoice={(messageId, value) => {
          if (messageId === PROVIDER_MESSAGE_ID) {
            setProviderResponseReady(false)
          }
          setState((prev) => ({
            ...prev,
            error: '',
            saved: false,
            choices: {
              ...prev.choices,
              [messageId]: value,
            },
          }))
        }}
        onMessageAnimationComplete={(messageId) => {
          setAnimatedCompleted((prev) => ({ ...prev, [messageId]: true }))
        }}
        onAction={(_, actionId) => {
          if (actionId !== CONNECT_ACTION_ID) return
          if (!isProviderSupported(selectedProvider)) return

          const apiKey = state.choices[PROVIDER_API_KEY_MESSAGE_ID] ?? ''
          const saveFailedMessage = i18n.t('onboard.error.save_failed', {
            provider: selectedProviderLabel || selectedProvider,
          })
          setState((prev) => ({ ...prev, saving: true, error: '', saved: false }))

          Effect.runPromise(saveProvider(selectedProvider, apiKey.trim(), saveFailedMessage))
            .then(() => {
              setState((prev) => ({ ...prev, saving: false, saved: true }))
            })
            .catch((error: unknown) => {
              const message = error instanceof Error ? error.message : saveFailedMessage
              setState((prev) => ({ ...prev, saving: false, error: message }))
            })
        }}
      />
    </div>
  )
}
