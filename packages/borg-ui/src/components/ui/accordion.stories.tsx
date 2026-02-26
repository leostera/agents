import type { Meta, StoryObj } from '@storybook/react'

import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from './accordion'

const meta: Meta<typeof Accordion> = {
  title: 'UI/Accordion',
  component: Accordion,
  args: {
    type: 'single',
    collapsible: true,
    defaultValue: 'item-1',
  },
}

export default meta
type Story = StoryObj<typeof Accordion>

export const Default: Story = {
  render: (args) => (
    <Accordion {...args}>
      <AccordionItem value='item-1'>
        <AccordionTrigger>What is included in onboarding?</AccordionTrigger>
        <AccordionContent>
          <p>Provider setup, API key validation, and a first successful session handoff.</p>
        </AccordionContent>
      </AccordionItem>
      <AccordionItem value='item-2'>
        <AccordionTrigger>How long does a new workspace take?</AccordionTrigger>
        <AccordionContent>
          <p>Most teams complete setup in under 10 minutes when credentials are ready.</p>
        </AccordionContent>
      </AccordionItem>
      <AccordionItem value='item-3'>
        <AccordionTrigger>Can we switch providers later?</AccordionTrigger>
        <AccordionContent>
          <p>Yes. Provider choice is session-scoped and can be changed without rebuilding the UI.</p>
        </AccordionContent>
      </AccordionItem>
    </Accordion>
  ),
}

export const Multiple: Story = {
  args: {
    type: 'multiple',
    defaultValue: ['item-1', 'item-2'],
  },
  render: (args) => (
    <Accordion {...args}>
      <AccordionItem value='item-1'>
        <AccordionTrigger>Runtime status</AccordionTrigger>
        <AccordionContent>
          <p>3 ports connected, 2 sessions active.</p>
        </AccordionContent>
      </AccordionItem>
      <AccordionItem value='item-2'>
        <AccordionTrigger>Pending tasks</AccordionTrigger>
        <AccordionContent>
          <p>Code review, onboarding copy update, and release note draft.</p>
        </AccordionContent>
      </AccordionItem>
    </Accordion>
  ),
}
