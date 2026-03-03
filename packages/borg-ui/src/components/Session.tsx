import { Icon } from "@iconify/react";
import React, { useMemo } from "react";

import type { SessionMessage } from "../lib/messages";
import { ChoiceInput } from "./ChoiceInput";
import { Message } from "./Message";
import { Spacer } from "./Spacer";
import { TextInput } from "./TextInput";

type SessionProps = {
  messages: Array<SessionMessage>;
  choices: Record<string, string>;
  animatedIds?: Array<string>;
  onChoice: (messageId: string, value: string) => void;
  onAction?: (messageId: string, actionId: string) => void;
  onMessageAnimationComplete?: (messageId: string) => void;
  assistantName?: string;
  systemName?: string;
  userName?: string;
};

export function Session(props: SessionProps) {
  const animated = useMemo(
    () => new Set(props.animatedIds ?? []),
    [props.animatedIds]
  );
  const getAuthorLabel = (author: "system" | "assistant" | "user") => {
    if (author === "assistant") return props.assistantName;
    if (author === "system") return props.systemName;
    return props.userName;
  };

  const composerMessage = useMemo(() => {
    for (let index = props.messages.length - 1; index >= 0; index -= 1) {
      const message = props.messages[index];
      if (
        message.type === "message" &&
        message.author === "user" &&
        (message.choices || message.input || message.actions)
      ) {
        return message;
      }
    }
    return null;
  }, [props.messages]);

  const feedMessages = useMemo(
    () =>
      composerMessage
        ? props.messages.filter((message) => message.id !== composerMessage.id)
        : props.messages,
    [composerMessage, props.messages]
  );

  const renderMessageControls = (
    message: Extract<SessionMessage, { type: "message" }>
  ) => (
    <>
      {message.choices && !animated.has(message.id) ? (
        <>
          <Spacer size={10} />
          <div className="borg-inline-choices">
            {message.choices.options.map((option) => (
              <button
                key={option.value}
                type="button"
                disabled={props.choices[message.id] === option.value}
                className={
                  props.choices[message.id] === option.value
                    ? "borg-inline-choice borg-inline-choice--active"
                    : "borg-inline-choice"
                }
                onClick={() => props.onChoice(message.id, option.value)}
              >
                <span className="borg-inline-choice__content">
                  <ChoiceIcon icon={option.icon} />
                  <span>{option.label}</span>
                </span>
              </button>
            ))}
          </div>
        </>
      ) : null}
      {message.input && !animated.has(message.id) ? (
        <>
          <Spacer size={10} />
          <div className="borg-session__input-body">
            <TextInput
              name={message.input.name}
              placeholder={message.input.placeholder}
              value={props.choices[message.input.id] ?? null}
              secret={message.input.secret}
              onChange={(value) => props.onChoice(message.input!.id, value)}
            />
          </div>
        </>
      ) : null}
      {message.actions && !animated.has(message.id) ? (
        <>
          <Spacer size={10} />
          <div className="borg-message__actions">
            {message.actions.map((action) => (
              <button
                key={action.id}
                type="button"
                className="borg-button borg-button--primary"
                disabled={action.disabled}
                onClick={() => props.onAction?.(message.id, action.id)}
              >
                {action.label}
              </button>
            ))}
          </div>
        </>
      ) : null}
    </>
  );

  const renderComposer = (
    message: Extract<SessionMessage, { type: "message" }>
  ) => {
    const primaryAction = message.actions?.[0];

    return (
      <div className="borg-composer">
        {message.choices ? (
          <div className="borg-inline-choices">
            {message.choices.options.map((option) => (
              <button
                key={option.value}
                type="button"
                disabled={props.choices[message.id] === option.value}
                className={
                  props.choices[message.id] === option.value
                    ? "borg-inline-choice borg-inline-choice--active"
                    : "borg-inline-choice"
                }
                onClick={() => props.onChoice(message.id, option.value)}
              >
                <span className="borg-inline-choice__content">
                  <ChoiceIcon icon={option.icon} />
                  <span>{option.label}</span>
                </span>
              </button>
            ))}
          </div>
        ) : null}

        {message.input ? (
          <div className="borg-composer__row">
            <input
              className="borg-composer__input"
              type={message.input.secret ? "password" : "text"}
              placeholder={message.input.placeholder}
              value={props.choices[message.input.id] ?? ""}
              onChange={(event) =>
                props.onChoice(message.input!.id, event.currentTarget.value)
              }
            />
            {primaryAction ? (
              <button
                type="button"
                className="borg-button borg-button--primary"
                disabled={primaryAction.disabled}
                onClick={() => props.onAction?.(message.id, primaryAction.id)}
              >
                {primaryAction.label}
              </button>
            ) : null}
          </div>
        ) : null}

        {!message.input && message.actions ? (
          <div className="borg-message__actions">
            {message.actions.map((action) => (
              <button
                key={action.id}
                type="button"
                className="borg-button borg-button--primary"
                disabled={action.disabled}
                onClick={() => props.onAction?.(message.id, action.id)}
              >
                {action.label}
              </button>
            ))}
          </div>
        ) : null}
      </div>
    );
  };

  return (
    <section className="borg-session">
      <div className="borg-session__feed">
        {feedMessages.map((message) => {
          if (message.type === "message") {
            return (
              <Message
                key={message.id}
                author={message.author}
                authorLabel={getAuthorLabel(message.author)}
                text={message.content}
                timestamp={message.timestamp}
                animate={animated.has(message.id)}
                onAnimationComplete={() =>
                  props.onMessageAnimationComplete?.(message.id)
                }
              >
                {renderMessageControls(message)}
              </Message>
            );
          }

          if (message.inputType === "choice") {
            return (
              <Message
                key={message.id}
                author={message.author}
                authorLabel={getAuthorLabel(message.author)}
                text={message.prompt}
                timestamp={message.timestamp}
              >
                <div className="borg-session__input-body">
                  <ChoiceInput
                    name={message.payload.name}
                    placeholder={message.payload.placeholder}
                    options={message.payload.options}
                    value={props.choices[message.id] ?? null}
                    onChange={(value) => props.onChoice(message.id, value)}
                  />
                </div>
              </Message>
            );
          }

          if (message.inputType === "text") {
            return (
              <Message
                key={message.id}
                author={message.author}
                authorLabel={getAuthorLabel(message.author)}
                text={message.prompt}
                timestamp={message.timestamp}
              >
                <div className="borg-session__input-body">
                  <TextInput
                    name={message.payload.name}
                    placeholder={message.payload.placeholder}
                    value={props.choices[message.id] ?? null}
                    secret={message.payload.secret}
                    onChange={(value) => props.onChoice(message.id, value)}
                  />
                </div>
              </Message>
            );
          }

          return null;
        })}
      </div>
      {composerMessage ? (
        <div className="borg-session__composer">
          {renderComposer(composerMessage)}
        </div>
      ) : null}
    </section>
  );
}

function ChoiceIcon({ icon }: { icon?: "openai" | "openrouter" }) {
  if (icon === "openai") return <OpenAiLogo />;
  if (icon === "openrouter") return <OpenRouterLogo />;
  return null;
}

function OpenAiLogo() {
  return (
    <Icon
      icon="streamline-logos:openai-logo-solid"
      className="borg-inline-choice__icon"
      width={16}
      height={16}
    />
  );
}

function OpenRouterLogo() {
  return (
    <Icon
      icon="simple-icons:openrouter"
      className="borg-inline-choice__icon"
      width={16}
      height={16}
    />
  );
}
