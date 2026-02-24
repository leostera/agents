import React from 'react'
import { useMemo, useState } from 'react'
import { Effect, pipe } from 'effect'

import { createI18n } from '@borg/i18n'
import { Session, type SessionMessage } from '@borg/ui'

const OPENAI_PROVIDER = 'openai'
const OPENAI_PROVIDER_KEY = 'openai_api_key'

type SessionState = {
  messages: Array<SessionMessage>
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

export function OnboardApp() {
  const i18n = useMemo(() => createI18n('en'), [])
  const [state, setState] = useState<SessionState>(() => {
    const username = getUsername()

    return {
      choices: {},
      saving: false,
      error: '',
      saved: false,
      messages: [
        {
          id: 'm-welcome',
          type: 'message',
          author: 'agent',
          content: i18n.t('onboard.agent.welcome', { username }),
        },
        {
          id: 'i-provider',
          type: 'input',
          inputType: 'choice',
          author: 'agent',
          prompt: i18n.t('onboard.agent.choose_provider'),
          payload: {
            name: i18n.t('onboard.choice.provider_name'),
            placeholder: i18n.t('onboard.choice.provider_placeholder'),
            options: [{ label: i18n.t('onboard.provider.openai'), value: OPENAI_PROVIDER }],
          },
        },
      ],
    }
  })

  const selectedProvider = state.choices['i-provider']
  const apiKeyMessage: SessionMessage | null = useMemo(() => {
    if (selectedProvider !== OPENAI_PROVIDER) return null

    return {
      id: 'm-key',
      type: 'message',
      author: 'agent',
      content: i18n.t('onboard.agent.openai_key_prompt'),
    }
  }, [i18n, selectedProvider])

  const messages = useMemo(() => {
    if (!apiKeyMessage) return state.messages
    return [...state.messages, apiKeyMessage]
  }, [apiKeyMessage, state.messages])

  return (
    <div>
      <h1 className='onboard-heading'>{i18n.t('onboard.title')}</h1>
      <p className='onboard-tagline'>{i18n.t('onboard.tagline')}</p>
      <Session
        messages={messages}
        choices={state.choices}
        animatedIds={['m-welcome']}
        onChoice={(messageId, value) => {
          setState((prev) => ({
            ...prev,
            choices: {
              ...prev.choices,
              [messageId]: value,
            },
          }))
        }}
      />

      {selectedProvider === OPENAI_PROVIDER ? (
        <section className='card' style={{ marginTop: 14 }}>
          <p className='step'>{i18n.t('onboard.user_turn')}</p>
          <label className='field-label' htmlFor='openai-api-key'>
            {i18n.t('onboard.field.api_key')}
          </label>
          <input
            id='openai-api-key'
            className='field-input'
            type='password'
            placeholder='sk-...'
            onChange={(event) => {
              setState((prev) => ({
                ...prev,
                choices: {
                  ...prev.choices,
                  [OPENAI_PROVIDER_KEY]: event.currentTarget.value,
                },
              }))
            }}
            value={state.choices[OPENAI_PROVIDER_KEY] ?? ''}
          />

          {state.error.length > 0 ? <p className='notice-error'>{state.error}</p> : null}
          {state.saved ? <p className='notice-success'>{i18n.t('onboard.notice.saved')}</p> : null}

          <div className='actions'>
            <button
              className='btn-primary'
              disabled={state.saving}
              onClick={() => {
                const apiKey = state.choices[OPENAI_PROVIDER_KEY] ?? ''
                setState((prev) => ({ ...prev, saving: true, error: '', saved: false }))

                Effect.runPromise(saveProvider(apiKey.trim(), i18n.t('onboard.error.save_failed')))
                  .then(() => {
                    setState((prev) => ({ ...prev, saving: false, saved: true }))
                  })
                  .catch((error: unknown) => {
                    const message = error instanceof Error ? error.message : i18n.t('onboard.error.save_failed')
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
