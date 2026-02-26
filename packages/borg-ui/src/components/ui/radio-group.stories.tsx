import type { Meta, StoryObj } from '@storybook/react'
import { useState } from 'react'

import { Field, FieldDescription, FieldLabel } from './field'
import { RadioGroup, RadioGroupItem } from './radio-group'

const meta: Meta<typeof RadioGroup> = {
  title: 'UI/RadioGroup',
  component: RadioGroup,
  args: {
    defaultValue: 'balanced',
  },
}

export default meta
type Story = StoryObj<typeof RadioGroup>

export const Default: Story = {
  render: (args) => {
    const [value, setValue] = useState('balanced')

    return (
      <RadioGroup {...args} value={value} onValueChange={setValue} style={{ width: 420 }}>
        <Field orientation='horizontal'>
          <RadioGroupItem id='mode-fast' value='fast' />
          <FieldLabel htmlFor='mode-fast'>
            <span>Fast</span>
            <FieldDescription>Lower latency, less context.</FieldDescription>
          </FieldLabel>
        </Field>
        <Field orientation='horizontal'>
          <RadioGroupItem id='mode-balanced' value='balanced' />
          <FieldLabel htmlFor='mode-balanced'>
            <span>Balanced</span>
            <FieldDescription>Recommended for most workflows.</FieldDescription>
          </FieldLabel>
        </Field>
        <Field orientation='horizontal'>
          <RadioGroupItem id='mode-deep' value='deep' />
          <FieldLabel htmlFor='mode-deep'>
            <span>Deep reasoning</span>
            <FieldDescription>Highest quality, slower response.</FieldDescription>
          </FieldLabel>
        </Field>
      </RadioGroup>
    )
  },
}
