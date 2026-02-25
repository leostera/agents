import React from 'react'

type TextInputProps = {
  name: string
  placeholder: string
  value: string | null
  secret?: boolean
  onChange: (value: string) => void
}

export function TextInput(props: TextInputProps) {
  return (
    <label className='borg-choice'>
      <span className='borg-choice__label'>{props.name}</span>
      <input
        className='borg-text-input'
        type={props.secret ? 'password' : 'text'}
        placeholder={props.placeholder}
        value={props.value ?? ''}
        onChange={(event) => props.onChange(event.currentTarget.value)}
      />
    </label>
  )
}
