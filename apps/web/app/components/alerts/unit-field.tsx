import { FieldRow } from '~/components/ui/field-row';
import { Input } from '~/components/ui/input';

type UnitFieldProps = {
  label: string;
  unit: string;
  value: string;
  onValueChange: (value: string) => void;
  placeholder?: string;
  isInvalid: boolean;
};

export function UnitField(props: UnitFieldProps) {
  const { label, unit, value, onValueChange, placeholder, isInvalid } = props;

  return (
    <FieldRow label={label}>
      {({ inputId }) => (
        <div className="relative">
          <Input
            id={inputId}
            inputMode="decimal"
            value={value}
            onValueChange={(next) => onValueChange(next.replace(/[^0-9.]/g, ''))}
            placeholder={placeholder}
            aria-invalid={isInvalid || undefined}
          />
          {unit && value && (
            <span aria-hidden className="pointer-events-none absolute inset-y-0 left-px flex items-center pl-3 text-body">
              <span className="invisible whitespace-pre">{value}</span>
              <span className="text-text-primary">{unit === '%' ? unit : ` ${unit}`}</span>
            </span>
          )}
        </div>
      )}
    </FieldRow>
  );
}
