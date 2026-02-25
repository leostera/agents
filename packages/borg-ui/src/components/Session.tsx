import React from 'react'
import { useMemo } from 'react'
import { Icon } from '@iconify/react'

import type { SessionMessage } from '../lib/messages'
import { ChoiceInput } from './ChoiceInput'
import { Message } from './Message'
import { Spacer } from './Spacer'
import { TextInput } from './TextInput'

type SessionProps = {
  messages: Array<SessionMessage>
  choices: Record<string, string>
  animatedIds?: Array<string>
  onChoice: (messageId: string, value: string) => void
  onMessageAnimationComplete?: (messageId: string) => void
  agentName?: string
  systemName?: string
  userName?: string
}

export function Session(props: SessionProps) {
  const animated = useMemo(() => new Set(props.animatedIds ?? []), [props.animatedIds])
  const getAuthorLabel = (author: 'system' | 'agent' | 'user') => {
    if (author === 'agent') return props.agentName
    if (author === 'system') return props.systemName
    return props.userName
  }

  return (
    <section className='borg-session'>
      {props.messages.map((message) => {
        if (message.type === 'message') {
          return (
            <Message
              key={message.id}
              author={message.author}
              authorLabel={getAuthorLabel(message.author)}
              text={message.content}
              timestamp={message.timestamp}
              animate={animated.has(message.id)}
              onAnimationComplete={() => props.onMessageAnimationComplete?.(message.id)}
            >
              {message.choices && !animated.has(message.id) ? (
                <>
                  <Spacer size={10} />
                  <div className='borg-inline-choices'>
                  {message.choices.options.map((option) => (
                      <button
                        key={option.value}
                        type='button'
                        disabled={props.choices[message.id] === option.value}
                        className={
                          props.choices[message.id] === option.value
                            ? 'borg-inline-choice borg-inline-choice--active'
                            : 'borg-inline-choice'
                        }
                        onClick={() => props.onChoice(message.id, option.value)}
                      >
                        <span className='borg-inline-choice__content'>
                          {option.icon === 'openai' ? <OpenAiLogo /> : null}
                          <span>{option.label}</span>
                        </span>
                      </button>
                    ))}
                  </div>
                </>
              ) : null}
            </Message>
          )
        }

        if (message.inputType === 'choice') {
          return (
            <article key={message.id} className='borg-session__input'>
              <Message
                author={message.author}
                authorLabel={getAuthorLabel(message.author)}
                text={message.prompt}
                timestamp={message.timestamp}
              />
              <div className='borg-session__input-body'>
                <ChoiceInput
                  name={message.payload.name}
                  placeholder={message.payload.placeholder}
                  options={message.payload.options}
                  value={props.choices[message.id] ?? null}
                  onChange={(value) => props.onChoice(message.id, value)}
                />
              </div>
            </article>
          )
        }

        if (message.inputType === 'text') {
          return (
            <article key={message.id} className='borg-session__input'>
              {message.prompt.trim().length > 0 ? (
                <Message
                  author={message.author}
                  authorLabel={getAuthorLabel(message.author)}
                  text={message.prompt}
                  timestamp={message.timestamp}
                />
              ) : null}
              <div className='borg-session__input-body'>
                <TextInput
                  name={message.payload.name}
                  placeholder={message.payload.placeholder}
                  value={props.choices[message.id] ?? null}
                  secret={message.payload.secret}
                  onChange={(value) => props.onChoice(message.id, value)}
                />
              </div>
            </article>
          )
        }

        return null
      })}
    </section>
  )
}

function OpenAiLogo() {
  return <Icon icon='streamline-logos:openai-logo-solid' className='borg-inline-choice__icon' width={16} height={16} />
}
