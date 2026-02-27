export type ProviderRecord = {
  provider: string
  api_key: string
  created_at: string
  updated_at: string
}

export type ProvidersResponse = {
  providers?: ProviderRecord[]
}

export type MemoryEntity = {
  entity_id: string
  entity_type: string
  label: string
  props?: Record<string, unknown>
}

export type MemorySearchResponse = {
  entities?: MemoryEntity[]
}

export class BorgApiError extends Error {
  status?: number
  bodyText?: string

  constructor(message: string, options: { status?: number; bodyText?: string } = {}) {
    super(message)
    this.name = 'BorgApiError'
    this.status = options.status
    this.bodyText = options.bodyText
  }
}

export type BorgApiClientOptions = {
  baseUrl?: string
}

function trimTrailingSlash(url: string): string {
  return url.replace(/\/+$/, '')
}

function resolveDefaultBaseUrl(): string {
  if (typeof window === 'undefined') return ''

  const fromEnv =
    (import.meta as unknown as { env?: Record<string, string | undefined> }).env?.VITE_BORG_API_BASE_URL ?? ''
  if (fromEnv.trim()) return trimTrailingSlash(fromEnv.trim())

  const { origin, hostname, protocol, port } = window.location
  const isLocal = hostname === 'localhost' || hostname === '127.0.0.1'
  if (isLocal && (port === '5173' || port === '4173')) {
    return `${protocol}//${hostname}:8080`
  }
  return origin
}

async function readResponseBody(response: Response): Promise<string> {
  try {
    return await response.text()
  } catch {
    return ''
  }
}

export class BorgApiClient {
  private readonly baseUrl: string

  constructor(options: BorgApiClientOptions = {}) {
    this.baseUrl = trimTrailingSlash(options.baseUrl ?? resolveDefaultBaseUrl())
  }

  private url(path: string): string {
    return `${this.baseUrl}${path}`
  }

  private async request(path: string, init?: RequestInit): Promise<Response> {
    const response = await fetch(this.url(path), init)
    if (!response.ok) {
      const bodyText = await readResponseBody(response)
      throw new BorgApiError(`Request failed (${response.status})`, {
        status: response.status,
        bodyText,
      })
    }
    return response
  }

  private async requestJson<T>(path: string, init?: RequestInit): Promise<T> {
    const response = await this.request(path, init)
    const contentType = response.headers.get('content-type') ?? ''
    if (!contentType.includes('application/json')) {
      const bodyText = await readResponseBody(response)
      throw new BorgApiError('Endpoint returned non-JSON response', {
        status: response.status,
        bodyText,
      })
    }
    return (await response.json()) as T
  }

  async listProviders(limit = 100): Promise<ProviderRecord[]> {
    const data = await this.requestJson<ProvidersResponse>(`/api/providers?limit=${limit}`)
    return Array.isArray(data.providers) ? data.providers : []
  }

  async upsertProviderApiKey(provider: string, apiKey: string): Promise<void> {
    await this.request(`/api/providers/${provider}`, {
      method: 'PUT',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ api_key: apiKey }),
    })
  }

  async deleteProvider(provider: string, options: { ignoreNotFound?: boolean } = {}): Promise<void> {
    try {
      await this.request(`/api/providers/${provider}`, { method: 'DELETE' })
    } catch (error) {
      if (
        options.ignoreNotFound &&
        error instanceof BorgApiError &&
        error.status === 404
      ) {
        return
      }
      throw error
    }
  }

  async startOpenAiDeviceCode(): Promise<void> {
    await this.request('/api/providers/openai/device-code/start', { method: 'POST' })
  }

  async searchMemory(params: { q: string; type?: string; limit?: number }): Promise<MemoryEntity[]> {
    const searchParams = new URLSearchParams({
      q: params.q,
      limit: String(params.limit ?? 50),
    })
    if (params.type && params.type.trim()) {
      searchParams.set('type', params.type.trim())
    }
    const data = await this.requestJson<MemorySearchResponse>(`/memory/search?${searchParams.toString()}`)
    return Array.isArray(data.entities) ? data.entities : []
  }
}

export function createBorgApiClient(options?: BorgApiClientOptions): BorgApiClient {
  return new BorgApiClient(options)
}
