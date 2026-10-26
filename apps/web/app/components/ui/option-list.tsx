import { Radio } from "@base-ui/react/radio";
import { RadioGroup } from "@base-ui/react/radio-group";

type Option<T extends string> = {
  value: T;
  title: string;
  description: string;
};

type OptionListProps<T extends string> = {
  label: string;
  value: T;
  options: Option<T>[];
  disabled?: boolean;
  onValueChange: (value: T) => void;
};

export function OptionList<T extends string>(props: OptionListProps<T>) {
  const { label, value, options, disabled, onValueChange } = props;

  return (
    <RadioGroup
      aria-label={label}
      value={value}
      disabled={disabled}
      onValueChange={(next) => onValueChange(next as T)}
      className="flex flex-col"
    >
      {options.map((option) => (
        <label
          key={option.value}
          className="flex cursor-pointer gap-2.5 px-4 py-3 has-data-disabled:cursor-default [&+&]:border-t [&+&]:border-border-row"
        >
          <Radio.Root
            value={option.value}
            className="mt-0.5 flex size-4 shrink-0 items-center justify-center rounded-full border border-border-strong bg-surface-default outline-none focus-visible:ring-2 focus-visible:ring-text-link/40 data-checked:border-button-primary data-checked:bg-button-primary"
          >
            <Radio.Indicator className="size-1.5 rounded-full bg-surface-default" />
          </Radio.Root>
          <span className="flex flex-col gap-0.5">
            <span className="text-body font-medium">{option.title}</span>
            <span className="text-body text-text-secondary">
              {option.description}
            </span>
          </span>
        </label>
      ))}
    </RadioGroup>
  );
}
