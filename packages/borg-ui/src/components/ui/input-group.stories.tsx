import type { Meta, StoryObj } from '@storybook/react'
import { MagnifyingGlassIcon } from '@phosphor-icons/react'

import {
  InputGroup,
  InputGroupAddon,
  InputGroupButton,
  InputGroupInput,
  InputGroupText,
  InputGroupTextarea,
} from './input-group'
import { Kbd } from './kbd'

const meta: Meta<typeof InputGroup> = {
  title: 'UI/InputGroup',
  component: InputGroup,
}

export default meta
type Story = StoryObj<typeof InputGroup>

export const Search: Story = {
  render: () => (
    <InputGroup style={{ width: 380 }}>
      <InputGroupAddon>
        <MagnifyingGlassIcon />
      </InputGroupAddon>
      <InputGroupInput placeholder='Search sessions...' />
      <InputGroupAddon align='inline-end'>
        <Kbd>/</Kbd>
      </InputGroupAddon>
    </InputGroup>
  ),
}

export const WithAction: Story = {
  render: () => (
    <InputGroup style={{ width: 420 }}>
      <InputGroupAddon>
        <InputGroupText>https://</InputGroupText>
      </InputGroupAddon>
      <InputGroupInput defaultValue='status.borg.dev' />
      <InputGroupAddon align='inline-end'>
        <InputGroupButton>Check</InputGroupButton>
      </InputGroupAddon>
    </InputGroup>
  ),
}

export const Multiline: Story = {
  render: () => (
    <InputGroup style={{ width: 420 }}>
      <InputGroupAddon align='block-start'>Notes</InputGroupAddon>
      <InputGroupTextarea rows={4} placeholder='Add deployment context...' />
    </InputGroup>
  ),
}
