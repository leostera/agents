import type { Meta, StoryObj } from '@storybook/react'

import {
  Field,
  FieldContent,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
  FieldLegend,
  FieldSet,
  FieldTitle,
} from './field'
import { Input } from './input'
import { Switch } from './switch'

const meta: Meta<typeof Field> = {
  title: 'UI/Field',
  component: Field,
}

export default meta
type Story = StoryObj<typeof Field>

export const Vertical: Story = {
  render: () => (
    <FieldGroup style={{ width: 420 }}>
      <Field>
        <FieldLabel htmlFor='workspace-name'>Workspace name</FieldLabel>
        <Input id='workspace-name' defaultValue='Acme Support' />
        <FieldDescription>Shown in shared links and notifications.</FieldDescription>
      </Field>
    </FieldGroup>
  ),
}

export const HorizontalWithError: Story = {
  render: () => (
    <FieldGroup style={{ width: 500 }}>
      <Field orientation='horizontal' data-invalid>
        <FieldLabel htmlFor='api-key'>API key</FieldLabel>
        <FieldContent>
          <Input id='api-key' aria-invalid defaultValue='sk-live-123' />
          <FieldError errors={[{ message: 'API key must start with sk-proj-' }]} />
        </FieldContent>
      </Field>
    </FieldGroup>
  ),
}

export const FieldSetExample: Story = {
  render: () => (
    <FieldSet style={{ width: 500 }}>
      <FieldLegend>Notifications</FieldLegend>
      <FieldDescription>Choose how Borg should notify on connection failures.</FieldDescription>
      <Field orientation='horizontal'>
        <FieldTitle>Email alerts</FieldTitle>
        <Switch defaultChecked aria-label='Email alerts' />
      </Field>
      <Field orientation='horizontal'>
        <FieldTitle>PagerDuty alerts</FieldTitle>
        <Switch aria-label='PagerDuty alerts' />
      </Field>
    </FieldSet>
  ),
}
