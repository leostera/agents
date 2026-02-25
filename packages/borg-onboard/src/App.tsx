import React from 'react'
import { useMemo, useState } from 'react'
import { Effect, pipe } from 'effect'

import { createI18n } from '@borg/i18n'
import { Session, type SessionMessage } from '@borg/ui'

const OPENAI_PROVIDER = 'openai'
const PROVIDER_MESSAGE_ID = 'm-welcome'
const OPENAI_API_KEY_MESSAGE_ID = 'i-openai-key'

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

function saveProvider(apiKey: string, saveFailedMessage: string) {
  return pipe(
    Effect.tryPromise(() =>
      fetch('/api/providers/openai', {
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
  const [keyPromptVisible, setKeyPromptVisible] = useState<boolean>(false)
  const [animatedCompleted, setAnimatedCompleted] = useState<Record<string, boolean>>({})

  const baseMessages = useMemo<Array<SessionMessage>>(
    () => [
      {
        id: 'm-welcome',
        type: 'message',
        author: 'agent',
        content: i18n.t('onboard.agent.welcome', { username }),
        timestamp: formatTimestamp(startedAt),
        choices: {
          name: i18n.t('onboard.choice.provider_name'),
          options: [{ label: i18n.t('onboard.provider.openai'), value: OPENAI_PROVIDER, icon: 'openai' }],
        },
      },
    ],
    [i18n, startedAt, username],
  )

  const selectedProvider = state.choices[PROVIDER_MESSAGE_ID]
  const messages = useMemo<Array<SessionMessage>>(() => {
    if (selectedProvider !== OPENAI_PROVIDER || !animatedCompleted['m-welcome']) return baseMessages

    const keyMessages: Array<SessionMessage> = [
      {
        id: 'm-key',
        type: 'message',
        author: 'agent',
        content: i18n.t('onboard.agent.openai_key_prompt'),
        timestamp: formatTimestamp(new Date(startedAt.getTime() + 60_000)),
      },
      {
        id: OPENAI_API_KEY_MESSAGE_ID,
        type: 'input',
        inputType: 'text',
        author: 'agent',
        prompt: '',
        timestamp: formatTimestamp(new Date(startedAt.getTime() + 60_000)),
        payload: {
          name: i18n.t('onboard.field.api_key'),
          placeholder: 'sk-...',
          secret: true,
        },
      },
    ]
    if (!keyPromptVisible) {
      return [...baseMessages, keyMessages[0]]
    }

    return [...baseMessages, ...keyMessages]
  }, [animatedCompleted, baseMessages, i18n, keyPromptVisible, selectedProvider])

  const animatedIds = useMemo(() => {
    const ids: Array<string> = []
    if (!animatedCompleted['m-welcome']) {
      ids.push('m-welcome')
    } else if (selectedProvider === OPENAI_PROVIDER && !animatedCompleted['m-key']) {
      ids.push('m-key')
    }
    return ids
  }, [animatedCompleted, selectedProvider])

  return (
    <div>
      <h1 className='onboard-heading' style={{ textAlign: 'center', marginBottom: 25 }}>
        Welcome to Borg
      </h1>
      <Session
        messages={messages}
        choices={state.choices}
        animatedIds={animatedIds}
        agentName='Borg'
        onChoice={(messageId, value) => {
          setState((prev) => ({
            ...prev,
            error: '',
            saved: false,
            choices: {
              ...prev.choices,
              [messageId]: value,
            },
          }))
          if (messageId === PROVIDER_MESSAGE_ID) {
            setKeyPromptVisible(false)
          }
        }}
        onMessageAnimationComplete={(messageId) => {
          setAnimatedCompleted((prev) => ({ ...prev, [messageId]: true }))
          if (messageId === 'm-key') {
            setKeyPromptVisible(true)
          }
        }}
      />

      {selectedProvider === OPENAI_PROVIDER && keyPromptVisible ? (
        <section className='card' style={{ marginTop: 15 }}>
          {state.error.length > 0 ? <p className='notice-error'>{state.error}</p> : null}
          {state.saved ? <p className='notice-success'>{i18n.t('onboard.notice.saved')}</p> : null}

          <div className='actions'>
            <button
              className='btn-primary'
              disabled={state.saving}
              onClick={() => {
                const apiKey = state.choices[OPENAI_API_KEY_MESSAGE_ID] ?? ''
                setState((prev) => ({ ...prev, saving: true, error: '', saved: false }))

                Effect.runPromise(saveProvider(apiKey.trim(), i18n.t('onboard.error.save_failed')))
                  .then(() => {
                    setState((prev) => ({ ...prev, saving: false, saved: true }))
                  })
                  .catch((error: unknown) => {
                    const message =
                      error instanceof Error ? error.message : i18n.t('onboard.error.save_failed')
                    setState((prev) => ({ ...prev, saving: false, error: message }))
                  })
              }}
            >
              {state.saving ? i18n.t('onboard.action.saving') : i18n.t('onboard.action.save_api_key')}
            </button>
          </div>
        </section>
      ) : null}
    </div>
  )
}
