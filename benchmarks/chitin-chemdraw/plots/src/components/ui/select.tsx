import { Select as Primitive } from "@kobalte/core/select";
import { createMemo } from "solid-js";

interface Choice<T> {
  key: string;
  label: string;
  value: T;
}

/** Solid shadcn composition: focus, typeahead, selection and portals belong to Kobalte. */
export function SelectField<T>(props: {
  label: string;
  options: T[];
  value: T | undefined;
  onChange: (value: T) => void;
  text: (value: T) => string;
  /** Object options can provide stable identity independently of their display text. */
  optionKey?: (value: T) => string;
}) {
  const key = (value: T) => props.optionKey?.(value) ?? (typeof value === "string" ? value : props.text(value));
  // Normalize strings too: primitive string collections otherwise use raw values
  // as item keys while controlled selection uses the custom display extractor.
  const choices = createMemo(() =>
    props.options.map((value) => ({ key: key(value), label: props.text(value), value })),
  );
  const selected = createMemo(() => {
    const value = props.value;
    return value === undefined ? undefined : choices().find((choice) => choice.key === key(value));
  });
  return (
    <Primitive<Choice<T>>
      options={choices()}
      value={selected()}
      onChange={(choice) => choice != null && props.onChange(choice.value)}
      optionValue="key"
      optionTextValue="label"
      itemComponent={(item) => (
        <Primitive.Item item={item.item} class="select-item">
          <Primitive.ItemLabel>{item.item.rawValue.label}</Primitive.ItemLabel>
          <Primitive.ItemIndicator>✓</Primitive.ItemIndicator>
        </Primitive.Item>
      )}
    >
      <Primitive.Label class="sr-only">{props.label}</Primitive.Label>
      <Primitive.Trigger class="select-trigger" aria-label={props.label}>
        <Primitive.Value<Choice<T>>>
          {(state) => {
            const selected = state.selectedOption();
            return selected ? selected.label : "Select a run";
          }}
        </Primitive.Value>
        <Primitive.Icon aria-hidden="true">⌄</Primitive.Icon>
      </Primitive.Trigger>
      <Primitive.Portal>
        <Primitive.Content class="select-content">
          <Primitive.Listbox />
        </Primitive.Content>
      </Primitive.Portal>
    </Primitive>
  );
}
