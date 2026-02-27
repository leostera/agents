import type { Meta, StoryObj } from "@storybook/react";
import { useState } from "react";

import { Field, FieldDescription, FieldLabel } from "./field";
import { Switch } from "./switch";

const meta: Meta<typeof Switch> = {
  title: "UI/Switch",
  component: Switch,
  args: {
    size: "default",
  },
};

export default meta;
type Story = StoryObj<typeof Switch>;

export const Default: Story = {
  render: (args) => {
    const [checked, setChecked] = useState(true);

    return (
      <Field orientation="horizontal" style={{ width: 380 }}>
        <FieldLabel htmlFor="session-logs">Session logs</FieldLabel>
        <Switch
          {...args}
          id="session-logs"
          checked={checked}
          onCheckedChange={setChecked}
          aria-label="Session logs"
        />
      </Field>
    );
  },
};

export const Small: Story = {
  args: {
    size: "sm",
    defaultChecked: true,
  },
};

export const WithDescription: Story = {
  render: () => (
    <Field orientation="horizontal" style={{ width: 420 }}>
      <FieldLabel htmlFor="auto-retry">Auto retry</FieldLabel>
      <FieldDescription>
        Retry failed provider connections up to 3 times.
      </FieldDescription>
      <Switch id="auto-retry" defaultChecked aria-label="Auto retry" />
    </Field>
  ),
};
