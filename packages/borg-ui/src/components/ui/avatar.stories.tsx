import type { Meta, StoryObj } from '@storybook/react'
import { CheckIcon } from '@phosphor-icons/react'

import {
  Avatar,
  AvatarBadge,
  AvatarFallback,
  AvatarGroup,
  AvatarGroupCount,
  AvatarImage,
} from './avatar'

const meta: Meta<typeof Avatar> = {
  title: 'UI/Avatar',
  component: Avatar,
  args: {
    size: 'default',
  },
}

export default meta
type Story = StoryObj<typeof Avatar>

export const Default: Story = {
  render: (args) => (
    <Avatar {...args}>
      <AvatarImage src='https://i.pravatar.cc/80?img=12' alt='Ari Chen' />
      <AvatarFallback>AC</AvatarFallback>
      <AvatarBadge>
        <CheckIcon />
      </AvatarBadge>
    </Avatar>
  ),
}

export const Group: Story = {
  render: () => (
    <AvatarGroup>
      <Avatar>
        <AvatarImage src='https://i.pravatar.cc/80?img=10' alt='Nora Singh' />
        <AvatarFallback>NS</AvatarFallback>
      </Avatar>
      <Avatar>
        <AvatarImage src='https://i.pravatar.cc/80?img=21' alt='Leo Park' />
        <AvatarFallback>LP</AvatarFallback>
      </Avatar>
      <Avatar>
        <AvatarImage src='https://i.pravatar.cc/80?img=38' alt='Dana Ruiz' />
        <AvatarFallback>DR</AvatarFallback>
      </Avatar>
      <AvatarGroupCount>+4</AvatarGroupCount>
    </AvatarGroup>
  ),
}
