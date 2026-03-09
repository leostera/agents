import type { Meta, StoryObj } from "@storybook/react-vite";
import { useState } from "react";
import { expect, userEvent, within } from "storybook/test";

import {
  Combobox,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxGroup,
  ComboboxInput,
  ComboboxItem,
  ComboboxLabel,
  ComboboxList,
  ComboboxSeparator,
} from "./combobox";

const frameworks = [
  "Next.js",
  "SvelteKit",
  "Nuxt.js",
  "Remix",
  "Astro",
  "SolidStart",
] as const;

const providerModels = [
  "openrouter/kimi-k2",
  "openrouter/claude-3.7-sonnet",
  "openrouter/gpt-4o-mini",
  "openrouter/qwen/qwen-2.5-coder-32b-instruct",
  "openai/gpt-4.1-mini",
  "openai/whisper-1",
] as const;

const meta: Meta<typeof Combobox> = {
  title: "UI/Combobox",
  component: Combobox,
};

export default meta;
type Story = StoryObj<typeof Combobox>;

export const FrameworkPicker: Story = {
  render: () => {
    const [value, setValue] = useState<(typeof frameworks)[number] | null>(
      "Next.js"
    );

    return (
      <div className="w-72 space-y-2">
        <Combobox
          items={frameworks}
          selectedValue={value}
          onSelectedValueChange={setValue}
        >
          <ComboboxInput placeholder="Pick a framework" showClear />
          <ComboboxContent>
            <ComboboxEmpty>No frameworks found.</ComboboxEmpty>
            <ComboboxList>
              {(item) => (
                <ComboboxItem key={item} value={item}>
                  {item}
                </ComboboxItem>
              )}
            </ComboboxList>
          </ComboboxContent>
        </Combobox>
        <p className="text-muted-foreground text-xs">
          Selected: {value ?? "None"}
        </p>
      </div>
    );
  },
};

export const GroupedOptions: Story = {
  render: () => {
    const [value, setValue] = useState<string | null>(null);

    return (
      <div className="w-72">
        <Combobox
          items={frameworks}
          selectedValue={value}
          onSelectedValueChange={setValue}
        >
          <ComboboxInput placeholder="Search stacks" showClear />
          <ComboboxContent>
            <ComboboxEmpty>No stacks found.</ComboboxEmpty>
            <ComboboxList>
              <ComboboxGroup>
                <ComboboxLabel>Popular</ComboboxLabel>
                <ComboboxItem value="Next.js">Next.js</ComboboxItem>
                <ComboboxItem value="Remix">Remix</ComboboxItem>
              </ComboboxGroup>
              <ComboboxSeparator />
              <ComboboxGroup>
                <ComboboxLabel>Emerging</ComboboxLabel>
                <ComboboxItem value="Astro">Astro</ComboboxItem>
                <ComboboxItem value="SolidStart">SolidStart</ComboboxItem>
              </ComboboxGroup>
            </ComboboxList>
          </ComboboxContent>
        </Combobox>
      </div>
    );
  },
};

export const ProviderModelPicker: Story = {
  render: () => {
    const [value, setValue] = useState<string | null>("openrouter/kimi-k2");

    return (
      <div className="w-96 space-y-2">
        <Combobox
          items={providerModels}
          selectedValue={value}
          onSelectedValueChange={setValue}
        >
          <ComboboxInput placeholder="Search and select model" showClear />
          <ComboboxContent>
            <ComboboxEmpty>No models found.</ComboboxEmpty>
            <ComboboxList>
              {(item) => (
                <ComboboxItem key={item} value={item}>
                  {item}
                </ComboboxItem>
              )}
            </ComboboxList>
          </ComboboxContent>
        </Combobox>
        <p className="text-muted-foreground text-xs">
          Selected model: {value ?? "None"}
        </p>
      </div>
    );
  },
};

export const ProviderModelPickerInDialogKeyboard: Story = {
  render: () => {
    const [value, setValue] = useState<string | null>(null);

    return (
      <div className="w-[42rem] rounded-xl border bg-background p-4 shadow-sm">
        <div className="mb-3">
          <h3 className="text-sm font-medium">Provider Model Picker</h3>
          <p className="text-muted-foreground text-xs">
            Type to filter, then use arrow keys and enter to select.
          </p>
        </div>
        <div className="space-y-2">
          <Combobox
            items={providerModels}
            selectedValue={value}
            onSelectedValueChange={setValue}
          >
            <ComboboxInput placeholder="Search and select model" showClear />
            <ComboboxContent className="max-h-80">
              <ComboboxEmpty>No models found.</ComboboxEmpty>
              <ComboboxList>
                {(item) => (
                  <ComboboxItem key={item} value={item}>
                    {item}
                  </ComboboxItem>
                )}
              </ComboboxList>
            </ComboboxContent>
          </Combobox>
          <p
            data-testid="selected-provider-model"
            className="text-muted-foreground text-xs"
          >
            Selected model: {value ?? "None"}
          </p>
        </div>
      </div>
    );
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const input = canvas.getByPlaceholderText("Search and select model");

    await userEvent.click(input);
    await userEvent.clear(input);
    await userEvent.type(input, "kimi");
    await userEvent.keyboard("{ArrowDown}{Enter}");

    await expect(
      canvas.getByTestId("selected-provider-model")
    ).toHaveTextContent("openrouter/kimi-k2");
  },
};
