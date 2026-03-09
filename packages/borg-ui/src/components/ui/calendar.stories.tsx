import type { Meta, StoryObj } from "@storybook/react-vite";
import { addDays } from "date-fns";
import { useState } from "react";
import type { DateRange } from "react-day-picker";

import { Calendar } from "./calendar";

const meta: Meta<typeof Calendar> = {
  title: "UI/Calendar",
  component: Calendar,
};

export default meta;
type Story = StoryObj<typeof Calendar>;

export const SingleDate: Story = {
  render: () => {
    const [selected, setSelected] = useState<Date | undefined>(new Date());

    return (
      <div className="w-fit rounded-xl border">
        <Calendar mode="single" selected={selected} onSelect={setSelected} />
      </div>
    );
  },
};

export const DateRangeSelection: Story = {
  render: () => {
    const [range, setRange] = useState<DateRange | undefined>({
      from: new Date(),
      to: addDays(new Date(), 4),
    });

    return (
      <div className="w-fit rounded-xl border">
        <Calendar
          mode="range"
          numberOfMonths={2}
          selected={range}
          onSelect={setRange}
          defaultMonth={range?.from}
        />
      </div>
    );
  },
};
