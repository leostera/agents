import type { Meta, StoryObj } from "@storybook/react";
import * as React from "react";

import {
  NativeSelect,
  NativeSelectOptGroup,
  NativeSelectOption,
} from "./native-select";

const meta: Meta<typeof NativeSelect> = {
  title: "UI/Native Select",
  component: NativeSelect,
};

export default meta;
type Story = StoryObj<typeof NativeSelect>;

export const ProviderPicker: Story = {
  render: () => (
    <NativeSelect defaultValue="openai">
      <NativeSelectOption value="openai">OpenAI</NativeSelectOption>
      <NativeSelectOption value="anthropic">Anthropic</NativeSelectOption>
      <NativeSelectOption value="google">Google</NativeSelectOption>
    </NativeSelect>
  ),
};

export const GroupedModels: Story = {
  render: () => {
    const [value, setValue] = React.useState("gpt-4.1-mini");

    return (
      <div className="grid gap-2 max-w-xs">
        <NativeSelect
          value={value}
          onChange={(event) => setValue(event.currentTarget.value)}
        >
          <NativeSelectOptGroup label="OpenAI">
            <NativeSelectOption value="gpt-4.1-mini">
              gpt-4.1-mini
            </NativeSelectOption>
            <NativeSelectOption value="gpt-4.1">gpt-4.1</NativeSelectOption>
          </NativeSelectOptGroup>
          <NativeSelectOptGroup label="Anthropic">
            <NativeSelectOption value="claude-3-7-sonnet">
              claude-3-7-sonnet
            </NativeSelectOption>
            <NativeSelectOption value="claude-3-5-haiku">
              claude-3-5-haiku
            </NativeSelectOption>
          </NativeSelectOptGroup>
        </NativeSelect>
        <p className="text-xs text-muted-foreground">Selected model: {value}</p>
      </div>
    );
  },
};

export const SmallDisabled: Story = {
  render: () => (
    <NativeSelect size="sm" disabled defaultValue="locked">
      <NativeSelectOption value="locked">
        Provider locked by policy
      </NativeSelectOption>
    </NativeSelect>
  ),
};
