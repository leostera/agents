export type MessageAuthor = "system" | "assistant" | "user";
export type MessageChoiceIcon = "openai" | "openrouter";
export type MessageChoiceOption = {
  label: string;
  value: string;
  icon?: MessageChoiceIcon;
};
export type MessageAction = {
  id: string;
  label: string;
  disabled?: boolean;
};

export type ActorMessage =
  | {
      id: string;
      type: "message";
      author: MessageAuthor;
      content: string;
      timestamp?: string;
      choices?: {
        name: string;
        options: Array<MessageChoiceOption>;
      };
      input?: {
        id: string;
        inputType: "text";
        name: string;
        placeholder: string;
        secret?: boolean;
      };
      actions?: Array<MessageAction>;
    }
  | {
      id: string;
      type: "input";
      inputType: "choice";
      author: Exclude<MessageAuthor, "user">;
      prompt: string;
      timestamp?: string;
      payload: {
        name: string;
        placeholder: string;
        options: Array<MessageChoiceOption>;
      };
    }
  | {
      id: string;
      type: "input";
      inputType: "text";
      author: Exclude<MessageAuthor, "user">;
      prompt: string;
      timestamp?: string;
      payload: {
        name: string;
        placeholder: string;
        secret?: boolean;
      };
    };
