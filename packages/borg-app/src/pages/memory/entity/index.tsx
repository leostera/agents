import React from 'react'
import { createBorgApiClient } from '@borg/api'
import { Card, CardContent, EntityView } from '@borg/ui'
import { LoaderCircle } from 'lucide-react'

type MemoryEntity = {
  entity_id: string
  entity_type: string
  label: string
  props?: Record<string, unknown>
}

type MemoryEntityPageProps = {
  entityUri: string
}

const borgApi = createBorgApiClient()
const TX_URI_RE = /^borg:tx:/i

export function MemoryEntityPage({ entityUri }: MemoryEntityPageProps) {
  const [isLoading, setIsLoading] = React.useState(false)
  const [error, setError] = React.useState<string | null>(null)
  const [entity, setEntity] = React.useState<MemoryEntity | null>(null)

  React.useEffect(() => {
    const loadEntity = async () => {
      const targetUri = entityUri.trim()
      if (!targetUri) {
        setEntity(null)
        setError('Missing entity URI')
        return
      }
      if (TX_URI_RE.test(targetUri)) {
        setEntity(null)
        setError('Transaction IDs are not entities')
        return
      }

      setIsLoading(true)
      setError(null)
      try {
        const found = (await borgApi.getMemoryEntity(targetUri)) as MemoryEntity | null
        if (!found) {
          setEntity(null)
          setError('Entity not found')
          return
        }
        setEntity(found)
      } catch (loadError) {
        setEntity(null)
        setError(loadError instanceof Error ? loadError.message : 'Unable to load entity')
      } finally {
        setIsLoading(false)
      }
    }

    void loadEntity()
  }, [entityUri])

  if (isLoading) {
    return (
      <p className='text-muted-foreground inline-flex items-center gap-2 text-xs'>
        <LoaderCircle className='size-4 animate-spin' />
        Loading entity...
      </p>
    )
  }

  if (error) {
    return <p className='text-destructive text-sm'>{error}</p>
  }

  if (!entity) {
    return <p className='text-muted-foreground text-sm'>No entity selected.</p>
  }

  return (
    <Card>
      <CardContent className='p-4'>
        <EntityView
          uri={entity.entity_id}
          kind={entity.entity_type}
          fields={entity.props}
        />
      </CardContent>
    </Card>
  )
}
