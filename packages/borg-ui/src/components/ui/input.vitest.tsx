import { render, screen } from "@testing-library/react";
import React from "react";
import { Input } from "./input";

describe("Input", () => {
  it("forwards refs to the underlying input", () => {
    const ref = React.createRef<HTMLInputElement>();
    render(<Input ref={ref} aria-label="provider api key" />);

    expect(ref.current).toBeInstanceOf(HTMLInputElement);
    ref.current?.focus();

    expect(screen.getByLabelText("provider api key")).toHaveFocus();
  });
});
