import React from 'react'
import { MemoryExplorer } from '@borg/explorer'

export function MemoryExplorerPage() {
  return (
    <section className='space-y-3'>
      <h2 className='text-lg font-semibold'>Memory Explorer</h2>
      <MemoryExplorer />
    </section>
  )
}
