import React, { useMemo, useState } from 'react'
import './styles.css'

type JsonMap = Record<string, unknown>

type MemoryEntity = {
  entity_id: string
  entity_type: string
  label: string
  props?: JsonMap
}

type GraphNode = {
  id: string
  label: string
  type: string
  x: number
  y: number
}

type GraphEdge = {
  source: string
  target: string
}

type MemoryExplorerProps = {
  className?: string
  initialQuery?: string
  initialType?: string
}

const URI_RE = /^[a-z][a-z0-9+.-]*:[^:\s]+:[^:\s]+$/i

function extractUriRefs(value: unknown, into: Set<string>) {
  if (typeof value === 'string') {
    if (URI_RE.test(value)) into.add(value)
    return
  }

  if (Array.isArray(value)) {
    for (const item of value) extractUriRefs(item, into)
    return
  }

  if (value && typeof value === 'object') {
    for (const nested of Object.values(value as JsonMap)) extractUriRefs(nested, into)
  }
}

function buildGraph(entities: MemoryEntity[]): { nodes: GraphNode[]; edges: GraphEdge[] } {
  const count = Math.max(entities.length, 1)
  const centerX = 360
  const centerY = 180
  const radius = Math.min(130 + count * 8, 240)

  const nodes = entities.map((entity, index) => {
    const angle = (index / count) * Math.PI * 2
    return {
      id: entity.entity_id,
      label: entity.label || entity.entity_id,
      type: entity.entity_type,
      x: centerX + Math.cos(angle) * radius,
      y: centerY + Math.sin(angle) * radius,
    }
  })

  const ids = new Set(nodes.map((node) => node.id))
  const edgeSet = new Set<string>()
  const edges: GraphEdge[] = []

  for (const entity of entities) {
    const refs = new Set<string>()
    extractUriRefs(entity.props ?? {}, refs)
    for (const ref of refs) {
      if (!ids.has(ref) || ref === entity.entity_id) continue
      const edgeKey = `${entity.entity_id}=>${ref}`
      if (edgeSet.has(edgeKey)) continue
      edgeSet.add(edgeKey)
      edges.push({ source: entity.entity_id, target: ref })
    }
  }

  return { nodes, edges }
}

async function fetchEntities(query: string, type: string, limit = 50): Promise<MemoryEntity[]> {
  const params = new URLSearchParams({ q: query, limit: String(limit) })
  if (type.trim()) params.set('type', type.trim())

  const response = await fetch(`/memory/search?${params.toString()}`, {
    headers: { accept: 'application/json' },
  })
  if (!response.ok) {
    throw new Error(`Memory search failed (${response.status})`)
  }

  const payload = (await response.json()) as { entities?: MemoryEntity[] }
  return Array.isArray(payload.entities) ? payload.entities : []
}

export function MemoryExplorer({ className, initialQuery = 'borg', initialType = '' }: MemoryExplorerProps) {
  const [query, setQuery] = useState(initialQuery)
  const [entityType, setEntityType] = useState(initialType)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')
  const [entities, setEntities] = useState<MemoryEntity[]>([])
  const [selectedId, setSelectedId] = useState('')

  async function runSearch(nextQuery = query, nextType = entityType) {
    setLoading(true)
    setError('')
    try {
      const rows = await fetchEntities(nextQuery.trim() || 'borg', nextType)
      setEntities(rows)
      setSelectedId(rows[0]?.entity_id ?? '')
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : 'Failed to search memory')
      setEntities([])
      setSelectedId('')
    } finally {
      setLoading(false)
    }
  }

  React.useEffect(() => {
    runSearch(initialQuery, initialType)
    // initial load only
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  const graph = useMemo(() => buildGraph(entities), [entities])
  const nodeById = useMemo(() => new Map(graph.nodes.map((node) => [node.id, node])), [graph.nodes])
  const selected = entities.find((entity) => entity.entity_id === selectedId) ?? null

  return (
    <section className={`explorer-root ${className ?? ''}`.trim()}>
      <div className='explorer-toolbar'>
        <input
          className='explorer-input'
          value={query}
          onChange={(event) => setQuery(event.currentTarget.value)}
          placeholder='Search memory entities'
          aria-label='Search memory entities'
        />
        <input
          className='explorer-input'
          value={entityType}
          onChange={(event) => setEntityType(event.currentTarget.value)}
          placeholder='Filter type (optional)'
          aria-label='Filter entity type'
        />
        <button className='explorer-button' onClick={() => runSearch()} disabled={loading}>
          {loading ? 'Loading...' : 'Search'}
        </button>
      </div>

      {error ? <p className='explorer-empty'>{error}</p> : null}

      <div className='explorer-content'>
        <div className='explorer-canvas'>
          {graph.nodes.length === 0 ? (
            <p className='explorer-empty'>No memory entities found for this query.</p>
          ) : (
            <svg width='100%' viewBox='0 0 720 360' role='img' aria-label='Memory graph explorer'>
              {graph.edges.map((edge) => {
                const source = nodeById.get(edge.source)
                const target = nodeById.get(edge.target)
                if (!source || !target) return null
                return (
                  <line
                    key={`${edge.source}-${edge.target}`}
                    x1={source.x}
                    y1={source.y}
                    x2={target.x}
                    y2={target.y}
                    stroke='rgba(148, 163, 184, 0.35)'
                    strokeWidth='1.2'
                  />
                )
              })}

              {graph.nodes.map((node) => {
                const isActive = node.id === selectedId
                return (
                  <g key={node.id} onClick={() => setSelectedId(node.id)} style={{ cursor: 'pointer' }}>
                    <circle
                      cx={node.x}
                      cy={node.y}
                      r={isActive ? 13 : 10}
                      fill={isActive ? '#0ea5e9' : '#22d3ee'}
                      fillOpacity={isActive ? 0.95 : 0.8}
                      stroke='rgba(2, 6, 23, 0.8)'
                      strokeWidth='2'
                    />
                    <text
                      x={node.x + 14}
                      y={node.y + 4}
                      fill='#cbd5e1'
                      fontSize='11'
                      fontFamily='ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace'
                    >
                      {node.label.slice(0, 32)}
                    </text>
                  </g>
                )
              })}
            </svg>
          )}
        </div>

        <aside className='explorer-side'>
          <div>
            <strong style={{ display: 'block', marginBottom: 6 }}>Entities ({entities.length})</strong>
            <div className='explorer-list'>
              {entities.slice(0, 40).map((entity) => (
                <button
                  key={entity.entity_id}
                  className={`explorer-item ${entity.entity_id === selectedId ? 'is-active' : ''}`}
                  onClick={() => setSelectedId(entity.entity_id)}
                >
                  {entity.label || entity.entity_id}
                  <small>{entity.entity_type}</small>
                </button>
              ))}
            </div>
          </div>

          <div>
            <strong style={{ display: 'block', marginBottom: 6 }}>Selected Entity</strong>
            {selected ? (
              <div className='explorer-meta'>
                <div><b>id:</b> {selected.entity_id}</div>
                <div><b>type:</b> {selected.entity_type}</div>
                <div><b>label:</b> {selected.label}</div>
                <hr style={{ borderColor: 'rgba(148, 163, 184, 0.2)' }} />
                <pre style={{ margin: 0, whiteSpace: 'pre-wrap' }}>
                  {JSON.stringify(selected.props ?? {}, null, 2)}
                </pre>
              </div>
            ) : (
              <p className='explorer-empty'>Select an entity to inspect memory properties.</p>
            )}
          </div>
        </aside>
      </div>
    </section>
  )
}
