import React from 'react'
import { useEffect, useMemo, useState } from 'react'

import type { MessageAuthor } from '../lib/messages'

type MessageProps = {
  author: MessageAuthor
  text: string
  animate?: boolean
  speedMs?: number
}

const DEFAULT_SPEED_MS = 12

export function Message(props: MessageProps) {
  const isRight = props.author === 'user'
  const [visibleText, setVisibleText] = useState(props.animate ? '' : props.text)

  useEffect(() => {
    if (!props.animate) {
      setVisibleText(props.text)
      return
    }

    setVisibleText('')
    let index = 0
    const timer = window.setInterval(() => {
      index += 1
      setVisibleText(props.text.slice(0, index))
      if (index >= props.text.length) {
        window.clearInterval(timer)
      }
    }, props.speedMs ?? DEFAULT_SPEED_MS)

    return () => {
      window.clearInterval(timer)
    }
  }, [props.animate, props.speedMs, props.text])

  const roleLabel = useMemo(() => props.author.toUpperCase(), [props.author])

  return (
    <article className={`borg-message-row ${isRight ? 'borg-message-row--right' : 'borg-message-row--left'}`}>
      <div className={`borg-message ${isRight ? 'borg-message--user' : 'borg-message--agent'}`}>
        <p className='borg-message__author'>{roleLabel}</p>
        <p className='borg-message__text'>{visibleText}</p>
      </div>
    </article>
  )
}
