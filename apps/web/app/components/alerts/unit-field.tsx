import { FieldRow } from '~/components/ui/field-row';
import { Input } from '~/components/ui/input';
import { getUnitSuffix } from '~/utils/format';

type UnitFieldProps = {
  label: string;
  unit: string;
  value: string;
  onValueChange: (value: string) => void;
  placeholder?: string;
  isInvalid: boolean;
  isSigned?: boolean;
};

function readTyped(text: string, isSigned: boolean) {
  const digits = text.replace(/[^0-9.]/g, '');
  if (isSigned && text.trimStart().startsWith('-')) {
    return `-${digits}`;
  }
  return digits;
}

export function UnitField(props: UnitFieldProps) {
  const { label, unit, value, onValueChange, placeholder, isInvalid, isSigned = false } = props;

  return (
    <FieldRow label={label}>
      {({ inputId }) => (
        <div className="relative">
          <Input
            id={inputId}
            inputMode="decimal"
            value={value}
            onValueChange={(next) => onValueChange(readTyped(next, isSigned))}
            placeholder={placeholder}
            aria-invalid={isInvalid || undefined}
          />
          {unit && value && (
            <span aria-hidden className="pointer-events-none absolute inset-y-0 left-px flex items-center pl-3 text-body">
              <span className="invisible whitespace-pre">{value}</span>
              <span className="whitespace-pre text-text-primary">{getUnitSuffix(unit)}</span>
            </span>
          )}
        </div>
      )}
    </FieldRow>
  );
}
