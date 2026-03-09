import type { Meta, StoryObj } from "@storybook/react-vite";
import { useState } from "react";

import { Slider } from "./slider";

const meta: Meta<typeof Slider> = {
  title: "UI/Slider",
  component: Slider,
  args: {
    min: 0,
    max: 100,
    step: 5,
  },
};

export default meta;
type Story = StoryObj<typeof Slider>;

export const SingleValue: Story = {
  render: (args) => {
    const [value, setValue] = useState([65]);

    return (
      <div style={{ width: 360, display: "grid", gap: 8 }}>
        <Slider {...args} value={value} onValueChange={setValue} />
        <span style={{ fontSize: 12 }}>Creativity: {value[0]}%</span>
      </div>
    );
  },
};

export const Range: Story = {
  render: (args) => {
    const [value, setValue] = useState([20, 80]);

    return (
      <div style={{ width: 360, display: "grid", gap: 8 }}>
        <Slider {...args} value={value} onValueChange={setValue} />
        <span style={{ fontSize: 12 }}>
          Active window: {value[0]} - {value[1]} minutes
        </span>
      </div>
    );
  },
};
