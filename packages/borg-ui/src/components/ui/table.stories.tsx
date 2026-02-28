import type { Meta, StoryObj } from "@storybook/react";

import {
  Table,
  TableBody,
  TableCaption,
  TableCell,
  TableFooter,
  TableHead,
  TableHeader,
  TableRow,
} from "./table";

const meta: Meta<typeof Table> = {
  title: "UI/Table",
  component: Table,
};

export default meta;
type Story = StoryObj<typeof Table>;

export const Default: Story = {
  render: () => (
    <Table>
      <TableCaption>Last 4 provider sync jobs.</TableCaption>
      <TableHeader>
        <TableRow>
          <TableHead>Provider</TableHead>
          <TableHead>Status</TableHead>
          <TableHead>Duration</TableHead>
          <TableHead className="text-right">Cost</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        <TableRow>
          <TableCell>OpenAI</TableCell>
          <TableCell>Success</TableCell>
          <TableCell>3m 21s</TableCell>
          <TableCell className="text-right">$0.42</TableCell>
        </TableRow>
        <TableRow data-state="selected">
          <TableCell>Anthropic</TableCell>
          <TableCell>Running</TableCell>
          <TableCell>1m 08s</TableCell>
          <TableCell className="text-right">$0.09</TableCell>
        </TableRow>
        <TableRow>
          <TableCell>Gemini</TableCell>
          <TableCell>Failed</TableCell>
          <TableCell>14s</TableCell>
          <TableCell className="text-right">$0.01</TableCell>
        </TableRow>
      </TableBody>
      <TableFooter>
        <TableRow>
          <TableCell colSpan={3}>Total</TableCell>
          <TableCell className="text-right">$0.52</TableCell>
        </TableRow>
      </TableFooter>
    </Table>
  ),
};
