import {
  DownloadIcon,
  FloppyDiskIcon,
  ShareNetworkIcon,
} from "@phosphor-icons/react";
import type { Meta, StoryObj } from "@storybook/react-vite";

import { Button } from "./button";
import {
  ButtonGroup,
  ButtonGroupSeparator,
  ButtonGroupText,
} from "./button-group";

const meta: Meta<typeof ButtonGroup> = {
  title: "UI/Button Group",
  component: ButtonGroup,
};

export default meta;
type Story = StoryObj<typeof ButtonGroup>;

export const HorizontalActions: Story = {
  render: () => (
    <ButtonGroup>
      <Button variant="secondary">
        <FloppyDiskIcon />
        Save
      </Button>
      <Button variant="secondary">
        <ShareNetworkIcon />
        Share
      </Button>
      <Button>
        <DownloadIcon />
        Publish
      </Button>
    </ButtonGroup>
  ),
};

export const MixedControls: Story = {
  render: () => (
    <ButtonGroup>
      <Button variant="outline">Preview</Button>
      <ButtonGroupSeparator />
      <ButtonGroupText>
        <span>Draft</span>
      </ButtonGroupText>
      <Button variant="default">Deploy</Button>
    </ButtonGroup>
  ),
};

export const Vertical: Story = {
  render: () => (
    <ButtonGroup orientation="vertical">
      <Button variant="outline">Top</Button>
      <Button variant="outline">Middle</Button>
      <Button variant="outline">Bottom</Button>
    </ButtonGroup>
  ),
};
