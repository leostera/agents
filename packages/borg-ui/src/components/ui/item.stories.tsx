import type { Meta, StoryObj } from '@storybook/react'
import { DatabaseIcon, KeyIcon, LinkIcon, RobotIcon } from '@phosphor-icons/react'

import { Badge } from './badge'
import { Button } from './button'
import {
  Item,
  ItemActions,
  ItemContent,
  ItemDescription,
  ItemFooter,
  ItemGroup,
  ItemHeader,
  ItemMedia,
  ItemSeparator,
  ItemTitle,
} from './item'

const meta: Meta<typeof Item> = {
  title: 'UI/Item',
  component: Item,
}

export default meta
type Story = StoryObj<typeof Item>

export const ProviderList: Story = {
  render: () => (
    <ItemGroup className='max-w-xl'>
      <Item variant='outline'>
        <ItemMedia variant='icon'>
          <RobotIcon />
        </ItemMedia>
        <ItemContent>
          <ItemHeader>
            <ItemTitle>OpenAI</ItemTitle>
            <Badge variant='secondary'>Connected</Badge>
          </ItemHeader>
          <ItemDescription>Primary inference provider for production sessions.</ItemDescription>
          <ItemFooter>
            <span className='text-muted-foreground text-xs'>Updated 2h ago</span>
            <ItemActions>
              <Button size='sm' variant='outline'>Rotate key</Button>
            </ItemActions>
          </ItemFooter>
        </ItemContent>
      </Item>
      <ItemSeparator />
      <Item variant='muted'>
        <ItemMedia variant='icon'>
          <LinkIcon />
        </ItemMedia>
        <ItemContent>
          <ItemTitle>Anthropic</ItemTitle>
          <ItemDescription>
            Standby fallback route for longer-context requests.
          </ItemDescription>
        </ItemContent>
        <ItemActions>
          <Button size='sm'>Connect</Button>
        </ItemActions>
      </Item>
    </ItemGroup>
  ),
}

export const CompactRows: Story = {
  render: () => (
    <ItemGroup className='max-w-lg'>
      <Item size='xs'>
        <ItemMedia variant='icon'>
          <KeyIcon />
        </ItemMedia>
        <ItemContent>
          <ItemTitle>API key present</ItemTitle>
          <ItemDescription>Stored in secure vault storage.</ItemDescription>
        </ItemContent>
      </Item>
      <Item size='xs'>
        <ItemMedia variant='icon'>
          <DatabaseIcon />
        </ItemMedia>
        <ItemContent>
          <ItemTitle>Session store ready</ItemTitle>
          <ItemDescription>SQLite persistence enabled.</ItemDescription>
        </ItemContent>
      </Item>
    </ItemGroup>
  ),
}
