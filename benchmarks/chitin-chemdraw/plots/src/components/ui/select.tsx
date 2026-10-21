import { Select as Primitive } from "@kobalte/core/select";

/** Solid shadcn composition: focus, typeahead, selection and portals belong to Kobalte. */
export function SelectField<T>(props: {
  label: string;
  options: T[];
  value: T | undefined;
  onChange: (value: T) => void;
  text: (value: T) => string;
}) {
  return (
    <Primitive<T>
      options={props.options}
      value={props.value}
      onChange={(value) => value !== null && props.onChange(value)}
      optionValue={(value) => props.text(value)}
      optionTextValue={props.text}
      itemComponent={(item) => (
        <Primitive.Item item={item.item} class="select-item">
          <Primitive.ItemLabel>{props.text(item.item.rawValue)}</Primitive.ItemLabel>
          <Primitive.ItemIndicator>✓</Primitive.ItemIndicator>
        </Primitive.Item>
      )}
    >
      <Primitive.Label class="sr-only">{props.label}</Primitive.Label>
      <Primitive.Trigger class="select-trigger" aria-label={props.label}>
        <Primitive.Value<T>>
          {(state) => {
            const selected = state.selectedOption();
            return selected ? props.text(selected) : "Select a run";
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
