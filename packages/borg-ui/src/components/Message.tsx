import React, { useEffect, useMemo, useState } from "react";

import type { MessageAuthor } from "../lib/messages";

type MessageProps = {
  author: MessageAuthor;
  text: string;
  timestamp?: string;
  authorLabel?: string;
  animate?: boolean;
  speedMs?: number;
  onAnimationComplete?: () => void;
  children?: React.ReactNode;
};

const DEFAULT_SPEED_MS = 12;

export function Message(props: MessageProps) {
  const isRight = props.author === "user";
  const [visibleText, setVisibleText] = useState(
    props.animate ? "" : props.text
  );

  useEffect(() => {
    if (!props.animate) {
      setVisibleText(props.text);
      return;
    }

    setVisibleText("");
    let index = 0;
    const timer = window.setInterval(() => {
      index += 1;
      setVisibleText(props.text.slice(0, index));
      if (index >= props.text.length) {
        window.clearInterval(timer);
        props.onAnimationComplete?.();
      }
    }, props.speedMs ?? DEFAULT_SPEED_MS);

    return () => {
      window.clearInterval(timer);
    };
  }, [props.animate, props.onAnimationComplete, props.speedMs, props.text]);

  const roleLabel = useMemo(
    () => (props.authorLabel ? props.authorLabel : props.author.toUpperCase()),
    [props.author, props.authorLabel]
  );

  return (
    <article
      className={`borg-message-row ${isRight ? "borg-message-row--right" : "borg-message-row--left"}`}
    >
      <div
        className={`borg-message ${isRight ? "borg-message--user" : "borg-message--assistant"}`}
      >
        <p className="borg-message__author">{roleLabel}</p>
        {visibleText.trim().length > 0 ? (
          <p className="borg-message__text">{visibleText}</p>
        ) : null}
        {props.children}
        {props.timestamp ? (
          <p className="borg-message__timestamp">{props.timestamp}</p>
        ) : null}
      </div>
    </article>
  );
}
