import type { Meta, StoryObj } from "@storybook/react-vite";
import { useState } from "react";

import {
  InputOTP,
  InputOTPGroup,
  InputOTPSeparator,
  InputOTPSlot,
} from "./input-otp";

const meta: Meta<typeof InputOTP> = {
  title: "UI/InputOTP",
  component: InputOTP,
  args: {
    maxLength: 6,
  },
};

export default meta;
type Story = StoryObj<typeof InputOTP>;

export const Default: Story = {
  render: (args) => {
    const [value, setValue] = useState("");

    return (
      <div style={{ display: "grid", gap: 8 }}>
        <InputOTP {...args} value={value} onChange={setValue}>
          <InputOTPGroup>
            <InputOTPSlot index={0} />
            <InputOTPSlot index={1} />
            <InputOTPSlot index={2} />
          </InputOTPGroup>
          <InputOTPSeparator />
          <InputOTPGroup>
            <InputOTPSlot index={3} />
            <InputOTPSlot index={4} />
            <InputOTPSlot index={5} />
          </InputOTPGroup>
        </InputOTP>
        <span style={{ fontSize: 12, color: "var(--muted-foreground)" }}>
          Code: {value || "------"}
        </span>
      </div>
    );
  },
};
