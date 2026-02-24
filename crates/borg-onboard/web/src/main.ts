import { Effect, pipe } from 'effect'

type State = {
  step: 1 | 2
  apiKey: string
  loading: boolean
  error: string
  saved: boolean
}

const state: State = {
  step: 1,
  apiKey: '',
  loading: false,
  error: '',
  saved: false,
}

const app = document.getElementById('app')

const saveProvider = (apiKey: string) =>
  pipe(
    Effect.tryPromise(() =>
      fetch('/api/providers/openai', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ api_key: apiKey }),
      }),
    ),
    Effect.flatMap((resp) => {
      if (!resp.ok) {
        return Effect.fail(new Error('Failed to save provider key'))
      }
      return Effect.tryPromise(() => resp.json())
    }),
  )

function render() {
  if (!app) return

  if (state.step === 1) {
    app.innerHTML = `
      <section class="rounded-xl border border-slate-800 bg-slate-900/70 p-6">
        <p class="text-xs uppercase tracking-wide text-slate-400">Step 1 of 2</p>
        <h2 class="mt-2 text-xl font-medium">Choose LLM Provider</h2>
        <button id="choose-openai" class="mt-6 w-full rounded-lg border border-emerald-400/50 bg-emerald-500/10 px-4 py-3 text-left">
          <span class="block text-sm font-medium">OpenAI (API key)</span>
          <span class="block text-xs text-slate-300 mt-1">Currently the only provider supported in onboarding.</span>
        </button>
      </section>
    `

    document.getElementById('choose-openai')?.addEventListener('click', () => {
      state.step = 2
      render()
    })
    return
  }

  app.innerHTML = `
    <section class="rounded-xl border border-slate-800 bg-slate-900/70 p-6">
      <p class="text-xs uppercase tracking-wide text-slate-400">Step 2 of 2</p>
      <h2 class="mt-2 text-xl font-medium">Enter OpenAI API Key</h2>
      <p class="mt-2 text-sm text-slate-300">This will be stored in <code>~/.borg/config.db</code> under <code>providers</code>.</p>
      <label class="mt-6 block text-sm">API Key</label>
      <input id="api-key" type="password" placeholder="sk-..." class="mt-2 w-full rounded-lg border border-slate-700 bg-slate-950 px-3 py-2 outline-none focus:border-emerald-400" value="${state.apiKey}" />
      ${state.error ? `<p class="mt-3 text-sm text-red-400">${state.error}</p>` : ''}
      ${state.saved ? `<p class="mt-3 text-sm text-emerald-400">Saved. You can now run <code>borg start</code>.</p>` : ''}
      <div class="mt-6 flex gap-3">
        <button id="back" class="rounded-lg border border-slate-700 px-4 py-2 text-sm">Back</button>
        <button id="save" class="rounded-lg bg-emerald-500 px-4 py-2 text-sm font-medium text-black disabled:opacity-50" ${state.loading ? 'disabled' : ''}>
          ${state.loading ? 'Saving...' : 'Save'}
        </button>
      </div>
    </section>
  `

  document.getElementById('api-key')?.addEventListener('input', (ev) => {
    state.apiKey = (ev.target as HTMLInputElement).value
  })

  document.getElementById('back')?.addEventListener('click', () => {
    state.step = 1
    state.error = ''
    state.saved = false
    render()
  })

  document.getElementById('save')?.addEventListener('click', () => {
    state.error = ''
    state.saved = false
    state.loading = true
    render()

    Effect.runPromise(saveProvider(state.apiKey))
      .then(() => {
        state.loading = false
        state.saved = true
        render()
      })
      .catch((err) => {
        state.loading = false
        state.error = err?.message || 'Unknown error'
        render()
      })
  })
}

render()
