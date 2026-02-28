import type { Meta, StoryObj } from "@storybook/react";
import { useState } from "react";

import {
  ContextMenu,
  ContextMenuCheckboxItem,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuRadioGroup,
  ContextMenuRadioItem,
  ContextMenuSeparator,
  ContextMenuShortcut,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
  ContextMenuTrigger,
} from "./context-menu";

const meta: Meta<typeof ContextMenu> = {
  title: "UI/Context Menu",
  component: ContextMenu,
};

export default meta;
type Story = StoryObj<typeof ContextMenu>;

export const FileContextMenu: Story = {
  render: () => {
    const [showLineNumbers, setShowLineNumbers] = useState(true);
    const [theme, setTheme] = useState("system");

    return (
      <ContextMenu>
        <ContextMenuTrigger className="bg-muted text-muted-foreground flex h-48 w-full max-w-md items-center justify-center rounded-xl border border-dashed text-sm">
          Right click this panel
        </ContextMenuTrigger>
        <ContextMenuContent className="w-56">
          <ContextMenuLabel>Editor</ContextMenuLabel>
          <ContextMenuItem>
            Rename
            <ContextMenuShortcut>F2</ContextMenuShortcut>
          </ContextMenuItem>
          <ContextMenuItem>
            Duplicate
            <ContextMenuShortcut>⌘D</ContextMenuShortcut>
          </ContextMenuItem>
          <ContextMenuSeparator />
          <ContextMenuCheckboxItem
            checked={showLineNumbers}
            onCheckedChange={(checked) => setShowLineNumbers(checked === true)}
          >
            Show line numbers
          </ContextMenuCheckboxItem>
          <ContextMenuSub>
            <ContextMenuSubTrigger>Theme</ContextMenuSubTrigger>
            <ContextMenuSubContent>
              <ContextMenuRadioGroup value={theme} onValueChange={setTheme}>
                <ContextMenuRadioItem value="light">Light</ContextMenuRadioItem>
                <ContextMenuRadioItem value="dark">Dark</ContextMenuRadioItem>
                <ContextMenuRadioItem value="system">
                  System
                </ContextMenuRadioItem>
              </ContextMenuRadioGroup>
            </ContextMenuSubContent>
          </ContextMenuSub>
        </ContextMenuContent>
      </ContextMenu>
    );
  },
};
