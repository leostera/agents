import React from 'react'
import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Input,
  Label,
} from '@borg/ui'
import { KeyRound, LoaderCircle } from 'lucide-react'

type ConnectProviderFormProps = {
  open: boolean
  onOpenChange: (open: boolean) => void
  isStartingOpenAi: boolean
  isSavingOpenRouter: boolean
  openRouterApiKey: string
  onOpenRouterApiKeyChange: (value: string) => void
  onStartOpenAiSignIn: () => void
  onSaveOpenRouter: (event: React.FormEvent<HTMLFormElement>) => void
}

export function ConnectProviderForm({
  open,
  onOpenChange,
  isStartingOpenAi,
  isSavingOpenRouter,
  openRouterApiKey,
  onOpenRouterApiKeyChange,
  onStartOpenAiSignIn,
  onSaveOpenRouter,
}: ConnectProviderFormProps) {
  const [showOpenRouterApiKeyForm, setShowOpenRouterApiKeyForm] = React.useState(false)

  React.useEffect(() => {
    if (!open) {
      setShowOpenRouterApiKeyForm(false)
    }
  }, [open])

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className='sm:max-w-lg'>
        <DialogHeader>
          <DialogTitle>Connect Provider</DialogTitle>
          <DialogDescription>Choose how you want to connect OpenAI or OpenRouter.</DialogDescription>
        </DialogHeader>

        <div className='space-y-4'>
          <div className='rounded-lg border px-4 py-3'>
            <div className='flex items-start justify-between gap-4'>
              <div className='min-w-0'>
                <p className='text-sm font-medium'>OpenAI</p>
                <p className='text-muted-foreground text-xs'>Use Codex device-code authentication.</p>
              </div>
              <Button variant='outline' onClick={onStartOpenAiSignIn} disabled={isStartingOpenAi}>
                {isStartingOpenAi ? <LoaderCircle className='size-4 animate-spin' /> : <KeyRound className='size-4' />}
                Sign in with OpenAI
              </Button>
            </div>
          </div>

          <div className='rounded-lg border px-4 py-3'>
            <div className='flex items-start justify-between gap-4'>
              <div className='min-w-0'>
                <p className='text-sm font-medium'>OpenRouter</p>
                <p className='text-muted-foreground text-xs'>Set an API key directly for OpenRouter requests.</p>
              </div>
              <Button variant='outline' onClick={() => setShowOpenRouterApiKeyForm((v) => !v)}>
                Use API Key
              </Button>
            </div>
            {showOpenRouterApiKeyForm ? (
              <form className='space-y-2' onSubmit={onSaveOpenRouter}>
                <Label htmlFor='openrouter-api-key'>OpenRouter API Key</Label>
                <Input
                  id='openrouter-api-key'
                  type='password'
                  autoComplete='off'
                  value={openRouterApiKey}
                  onChange={(event) => onOpenRouterApiKeyChange(event.currentTarget.value)}
                  placeholder='sk-or-v1-...'
                />
                <Button type='submit' disabled={isSavingOpenRouter}>
                  {isSavingOpenRouter ? <LoaderCircle className='size-4 animate-spin' /> : null}
                  Save API Key
                </Button>
              </form>
            ) : null}
          </div>
        </div>

        <DialogFooter showCloseButton />
      </DialogContent>
    </Dialog>
  )
}
