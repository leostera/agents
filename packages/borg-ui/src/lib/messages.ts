export type MessageAuthor = 'system' | 'agent' | 'user'
export type MessageChoiceOption = { label: string; value: string }

export type SessionMessage =
  | {
      id: string
      type: 'message'
      author: MessageAuthor
      content: string
      choices?: {
        name: string
        options: Array<MessageChoiceOption>
      }
    }
  | {
      id: string
      type: 'input'
      inputType: 'choice'
      author: Exclude<MessageAuthor, 'user'>
      prompt: string
      payload: {
        name: string
        placeholder: string
        options: Array<{ label: string; value: string }>
      }
    }
  | {
      id: string
      type: 'input'
      inputType: 'text'
      author: Exclude<MessageAuthor, 'user'>
      prompt: string
      payload: {
        name: string
        placeholder: string
        secret?: boolean
      }
    }
