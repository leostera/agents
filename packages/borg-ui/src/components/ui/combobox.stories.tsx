import type { Meta, StoryObj } from '@storybook/react'
import { useState } from 'react'

import {
  Combobox,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxGroup,
  ComboboxInput,
  ComboboxItem,
  ComboboxLabel,
  ComboboxList,
  ComboboxSeparator,
} from './combobox'

const frameworks = [
  'Next.js',
  'SvelteKit',
  'Nuxt.js',
  'Remix',
  'Astro',
  'SolidStart',
] as const

const meta: Meta<typeof Combobox> = {
  title: 'UI/Combobox',
  component: Combobox,
}

export default meta
type Story = StoryObj<typeof Combobox>

export const FrameworkPicker: Story = {
  render: () => {
    const [value, setValue] = useState<(typeof frameworks)[number] | null>(
      'Next.js'
    )

    return (
      <div className='w-72 space-y-2'>
        <Combobox
          items={frameworks}
          selectedValue={value}
          onSelectedValueChange={setValue}
        >
          <ComboboxInput placeholder='Pick a framework' showClear />
          <ComboboxContent>
            <ComboboxEmpty>No frameworks found.</ComboboxEmpty>
            <ComboboxList>
              {(item) => (
                <ComboboxItem key={item} value={item}>
                  {item}
                </ComboboxItem>
              )}
            </ComboboxList>
          </ComboboxContent>
        </Combobox>
        <p className='text-muted-foreground text-xs'>
          Selected: {value ?? 'None'}
        </p>
      </div>
    )
  },
}

export const GroupedOptions: Story = {
  render: () => {
    const [value, setValue] = useState<string | null>(null)

    return (
      <div className='w-72'>
        <Combobox
          items={frameworks}
          selectedValue={value}
          onSelectedValueChange={setValue}
        >
          <ComboboxInput placeholder='Search stacks' showClear />
          <ComboboxContent>
            <ComboboxEmpty>No stacks found.</ComboboxEmpty>
            <ComboboxList>
              <ComboboxGroup>
                <ComboboxLabel>Popular</ComboboxLabel>
                <ComboboxItem value='Next.js'>Next.js</ComboboxItem>
                <ComboboxItem value='Remix'>Remix</ComboboxItem>
              </ComboboxGroup>
              <ComboboxSeparator />
              <ComboboxGroup>
                <ComboboxLabel>Emerging</ComboboxLabel>
                <ComboboxItem value='Astro'>Astro</ComboboxItem>
                <ComboboxItem value='SolidStart'>SolidStart</ComboboxItem>
              </ComboboxGroup>
            </ComboboxList>
          </ComboboxContent>
        </Combobox>
      </div>
    )
  },
}
