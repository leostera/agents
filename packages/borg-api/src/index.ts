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

export type MemoryEntityResponse = {
  entity?: MemoryEntity
}

export type MemoryExplorerEdge = {
  source: string
  target: string
  relation: string
}

export type MemoryExplorerResponse = {
  entities?: MemoryEntity[]
  edges?: MemoryExplorerEdge[]
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
    let response: Response
    try {
      response = await fetch(this.url(path), init)
    } catch (error) {
      const fallbackOrigin = typeof window !== 'undefined' ? window.location.origin : ''
      const message =
        error instanceof TypeError
          ? `Borg API unavailable at ${this.baseUrl || fallbackOrigin}`
          : 'Unable to reach Borg API'
      throw new BorgApiError(message)
    }
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

  async getMemoryEntity(uri: string): Promise<MemoryEntity | null> {
    try {
      const data = await this.requestJson<MemoryEntityResponse>(`/memory/entities/${encodeURIComponent(uri)}`)
      return data.entity ?? null
    } catch (error) {
      if (error instanceof BorgApiError && error.status === 404) {
        return null
      }
      throw error
    }
  }

  async exploreMemory(params: {
    query: string
    limit?: number
    maxNodes?: number
  }): Promise<{ entities: MemoryEntity[]; edges: MemoryExplorerEdge[] }> {
    const searchParams = new URLSearchParams({
      query: params.query.trim(),
      limit: String(params.limit ?? 25),
      max_nodes: String(params.maxNodes ?? 300),
    })
    const data = await this.requestJson<MemoryExplorerResponse>(`/api/memory/explorer?${searchParams.toString()}`)
    return {
      entities: Array.isArray(data.entities) ? data.entities : [],
      edges: Array.isArray(data.edges) ? data.edges : [],
    }
  }
}

export function createBorgApiClient(options?: BorgApiClientOptions): BorgApiClient {
  return new BorgApiClient(options)
}
