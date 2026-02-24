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
      <section class="card">
        <p class="step">Step 1 of 2</p>
        <h2 class="card-title">Choose LLM Provider</h2>
        <button id="choose-openai" class="btn-provider">
          <span class="btn-provider-title">OpenAI (API key)</span>
          <span class="btn-provider-note">Currently the only provider supported in onboarding.</span>
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
    <section class="card">
      <p class="step">Step 2 of 2</p>
      <h2 class="card-title">Enter OpenAI API Key</h2>
      <p class="card-note">This will be stored in <code>~/.borg/config.db</code> under <code>providers</code>.</p>
      <label class="field-label">API Key</label>
      <input id="api-key" type="password" placeholder="sk-..." class="field-input" value="${state.apiKey}" />
      ${state.error ? `<p class="notice-error">${state.error}</p>` : ''}
      ${state.saved ? `<p class="notice-success">Saved. You can now run <code>borg start</code>.</p>` : ''}
      <div class="actions">
        <button id="back" class="btn-secondary">Back</button>
        <button id="save" class="btn-primary" ${state.loading ? 'disabled' : ''}>
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
