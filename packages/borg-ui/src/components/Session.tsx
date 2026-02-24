import React from 'react'
import { useMemo } from 'react'

import type { SessionMessage } from '../lib/messages'
import { ChoiceInput } from './ChoiceInput'
import { Message } from './Message'

type SessionProps = {
  messages: Array<SessionMessage>
  choices: Record<string, string>
  animatedIds?: Array<string>
  onChoice: (messageId: string, value: string) => void
}

export function Session(props: SessionProps) {
  const animated = useMemo(() => new Set(props.animatedIds ?? []), [props.animatedIds])

  return (
    <section className='borg-session'>
      {props.messages.map((message) => {
        if (message.type === 'message') {
          return (
            <Message
              key={message.id}
              author={message.author}
              text={message.content}
              animate={animated.has(message.id)}
            />
          )
        }

        if (message.inputType === 'choice') {
          return (
            <article key={message.id} className='borg-session__input'>
              <Message author={message.author} text={message.prompt} />
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

        return null
      })}
    </section>
  )
}
