import type { Meta, StoryObj } from "@storybook/react";
import { useState } from "react";
import { Checkbox } from "./checkbox";
import { Label } from "./label";

const meta: Meta<typeof Checkbox> = {
  title: "UI/Checkbox",
  component: Checkbox,
  args: {
    checked: true,
    disabled: false,
  },
};

export default meta;
type Story = StoryObj<typeof Checkbox>;

export const Default: Story = {
  render: (args) => {
    const [checked, setChecked] = useState(Boolean(args.checked));

    return (
      <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
        <Checkbox
          id="notify-build"
          checked={checked}
          onCheckedChange={(value) => setChecked(value === true)}
          disabled={args.disabled}
        />
        <Label htmlFor="notify-build">Notify me when the build completes</Label>
      </div>
    );
  },
};

export const Disabled: Story = {
  args: {
    checked: false,
    disabled: true,
  },
  render: (args) => (
    <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
      <Checkbox id="read-only" {...args} />
      <Label htmlFor="read-only">Read-only permission</Label>
    </div>
  ),
};
