import {
  FileIcon,
  GearIcon,
  KeyboardIcon,
  MoonIcon,
  UserIcon,
} from "@phosphor-icons/react";
import type { Meta, StoryObj } from "@storybook/react";
import { useState } from "react";

import {
  Command,
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
  CommandSeparator,
  CommandShortcut,
} from "./command";

const meta: Meta<typeof Command> = {
  title: "UI/Command",
  component: Command,
};

export default meta;
type Story = StoryObj<typeof Command>;

export const InlinePalette: Story = {
  render: () => {
    const [selected, setSelected] = useState("profile");

    return (
      <div className="w-full max-w-md rounded-xl border">
        <Command>
          <CommandInput placeholder="Search commands..." />
          <CommandList>
            <CommandEmpty>No results found.</CommandEmpty>
            <CommandGroup heading="Quick Actions">
              <CommandItem
                value="profile"
                data-checked={selected === "profile"}
                onSelect={() => setSelected("profile")}
              >
                <UserIcon />
                Open Profile
                <CommandShortcut>⌘P</CommandShortcut>
              </CommandItem>
              <CommandItem
                value="preferences"
                data-checked={selected === "preferences"}
                onSelect={() => setSelected("preferences")}
              >
                <GearIcon />
                Preferences
                <CommandShortcut>⌘,</CommandShortcut>
              </CommandItem>
            </CommandGroup>
            <CommandSeparator />
            <CommandGroup heading="Workspace">
              <CommandItem value="new-file">
                <FileIcon />
                New File
              </CommandItem>
              <CommandItem value="shortcuts">
                <KeyboardIcon />
                Keyboard Shortcuts
              </CommandItem>
              <CommandItem value="theme">
                <MoonIcon />
                Switch Theme
              </CommandItem>
            </CommandGroup>
          </CommandList>
        </Command>
      </div>
    );
  },
};

export const InsideDialog: Story = {
  render: () => (
    <div className="min-h-[420px] rounded-xl border bg-muted/20">
      <CommandDialog open onOpenChange={() => {}}>
        <CommandInput placeholder="Search commands..." />
        <CommandList>
          <CommandGroup heading="General">
            <CommandItem value="dashboard">Open Dashboard</CommandItem>
            <CommandItem value="billing">Open Billing</CommandItem>
          </CommandGroup>
        </CommandList>
      </CommandDialog>
    </div>
  ),
};
