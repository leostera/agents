import React from 'react'
import { CommandDialog, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList } from '@borg/ui'

export type CommandSection = {
  id: string
  title: string
  icon: React.ComponentType<{ className?: string }>
}

export type CommandSectionGroup = {
  id: string
  title: string
  items: CommandSection[]
}

type CommandKProps = {
  open: boolean
  onOpenChange: (open: boolean) => void
  groups: CommandSectionGroup[]
  onSelectSection: (sectionId: string) => void
}

export function CommandK({ open, onOpenChange, groups, onSelectSection }: CommandKProps) {
  return (
    <CommandDialog open={open} onOpenChange={onOpenChange}>
      <CommandInput placeholder='Search sections, observability, memory...' />
      <CommandList>
        <CommandEmpty>No results found.</CommandEmpty>
        {groups.map((group) => (
          <CommandGroup key={group.id} heading={group.title}>
            {group.items.map((section) => {
              const Icon = section.icon
              return (
                <CommandItem
                  key={section.id}
                  value={`${group.title} ${section.title}`}
                  onSelect={() => onSelectSection(section.id)}
                >
                  <Icon className='size-4' />
                  <span>{section.title}</span>
                </CommandItem>
              )
            })}
          </CommandGroup>
        ))}
      </CommandList>
    </CommandDialog>
  )
}
