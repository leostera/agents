import React from 'react'
import { useMemo } from 'react'

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
              <Message
                author={message.author}
                authorLabel={getAuthorLabel(message.author)}
                text={message.prompt}
                timestamp={message.timestamp}
              />
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
  return (
    <svg
      className='borg-inline-choice__icon'
      viewBox='0 0 24 24'
      width='16'
      height='16'
      fill='none'
      xmlns='http://www.w3.org/2000/svg'
      aria-hidden='true'
    >
      <path
        d='M12 3.5a2.8 2.8 0 0 1 2.8 2.8v.3l.3.1a2.8 2.8 0 0 1 1.7 4.1l-.1.2.2.2a2.8 2.8 0 0 1-.2 4.2l-.2.1.1.2a2.8 2.8 0 0 1-2.5 4.1h-.3l-.1.3a2.8 2.8 0 0 1-5.1 0l-.1-.3h-.3a2.8 2.8 0 0 1-2.5-4.1l.1-.2-.2-.1A2.8 2.8 0 0 1 5.1 11l.2-.2-.1-.2a2.8 2.8 0 0 1 1.7-4.1l.3-.1v-.3A2.8 2.8 0 0 1 10 3.5h2Zm-1.2 2.1a1.1 1.1 0 0 0-1.1 1.1v1.1L8.5 9a1.1 1.1 0 0 0-.3 1.3l.5 1-1 .5a1.1 1.1 0 0 0-.1 2l1 .5-.5 1a1.1 1.1 0 0 0 .3 1.3l1.2 1.1v1.1a1.1 1.1 0 0 0 2 0v-1.1l1.2-1.1a1.1 1.1 0 0 0 .3-1.3l-.5-1 1-.5a1.1 1.1 0 0 0-.1-2l-1-.5.5-1a1.1 1.1 0 0 0-.3-1.3l-1.2-1.1V6.7a1.1 1.1 0 0 0-1.1-1.1h-.6Z'
        fill='currentColor'
      />
    </svg>
  )
}
