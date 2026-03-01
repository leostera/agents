import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import React from "react";
import {
  Combobox,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxInput,
  ComboboxItem,
  ComboboxList,
} from "./combobox";

const models = [
  "openrouter/kimi-k2",
  "openrouter/claude-3.7-sonnet",
  "openai/gpt-4.1-mini",
] as const;

function Harness({ selectedValue }: { selectedValue?: string | null }) {
  const [value, setValue] = React.useState<string | null>(
    selectedValue ?? null
  );

  return (
    <div>
      <Combobox
        items={models}
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
      <p data-testid="selected-value">{value ?? "none"}</p>
    </div>
  );
}

describe("Combobox", () => {
  it("renders input and placeholder", () => {
    render(<Harness selectedValue="openrouter/kimi-k2" />);

    const input = screen.getByPlaceholderText("Search and select model");
    expect(input).toBeInTheDocument();
  });

  it("accepts typed input", async () => {
    const user = userEvent.setup();
    render(<Harness />);

    const input = screen.getByPlaceholderText("Search and select model");
    await user.click(input);
    await user.type(input, "kimi");

    expect(input).toHaveValue("kimi");
  });

  it("selects an option and updates selected value", async () => {
    const user = userEvent.setup();
    render(<Harness />);

    const input = screen.getByPlaceholderText("Search and select model");
    await user.click(input);
    await user.type(input, "kimi");
    await user.keyboard("{ArrowDown}{Enter}");

    expect(screen.getByTestId("selected-value")).toHaveTextContent(
      "openrouter/kimi-k2"
    );
  });
});
