import { SelectField } from '~/components/alerts/select-field';
import type { ConditionKind } from '~/utils/alerts';
import { FOR_OPTIONS } from '~/utils/alerts';

type ForFieldProps = {
  kind: ConditionKind;
  value: number;
  onValueChange: (value: number) => void;
};

export function ForField(props: ForFieldProps) {
  const { kind, value, onValueChange } = props;

  return (
    <SelectField
      label={kind === 'offline' ? 'No Data For' : 'For'}
      value={value.toString()}
      onValueChange={(next) => onValueChange(Number(next))}
      items={FOR_OPTIONS.map((option) => ({ value: option.value.toString(), label: option.label }))}
    />
  );
}
