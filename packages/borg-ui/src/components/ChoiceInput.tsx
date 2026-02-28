import React from "react";

type ChoiceOption = {
  label: string;
  value: string;
};

type ChoiceInputProps = {
  name: string;
  placeholder: string;
  options: Array<ChoiceOption>;
  value: string | null;
  onChange: (value: string) => void;
};

export function ChoiceInput(props: ChoiceInputProps) {
  return (
    <label className="borg-choice">
      <span className="borg-choice__label">{props.name}</span>
      <select
        className="borg-choice__select"
        value={props.value ?? ""}
        onChange={(event) => props.onChange(event.currentTarget.value)}
      >
        <option value="">{props.placeholder}</option>
        {props.options.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
    </label>
  );
}
