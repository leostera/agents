export type MessageAuthor = 'system' | 'agent' | 'user'

export type SessionMessage =
  | {
      id: string
      type: 'message'
      author: MessageAuthor
      content: string
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
