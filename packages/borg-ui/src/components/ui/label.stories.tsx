import type { Meta, StoryObj } from "@storybook/react";

import { Input } from "./input";
import { Label } from "./label";

const meta: Meta<typeof Label> = {
  title: "UI/Label",
  component: Label,
  args: {
    children: "Email",
  },
};

export default meta;
type Story = StoryObj<typeof Label>;

export const Default: Story = {};

export const WithControl: Story = {
  render: (args) => (
    <div style={{ width: 320, display: "grid", gap: 8 }}>
      <Label htmlFor="owner-email" {...args} />
      <Input id="owner-email" type="email" placeholder="owner@company.com" />
    </div>
  ),
};
