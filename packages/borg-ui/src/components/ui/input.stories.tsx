import type { Meta, StoryObj } from "@storybook/react";

import { Input } from "./input";

const meta: Meta<typeof Input> = {
  title: "UI/Input",
  component: Input,
  args: {
    placeholder: "name@company.com",
    type: "email",
  },
};

export default meta;
type Story = StoryObj<typeof Input>;

export const Default: Story = {};

export const Invalid: Story = {
  args: {
    "aria-invalid": true,
    defaultValue: "invalid-email",
  },
};

export const Disabled: Story = {
  args: {
    disabled: true,
    defaultValue: "disabled@company.com",
  },
};
