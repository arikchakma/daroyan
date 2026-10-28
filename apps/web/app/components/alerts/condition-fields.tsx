import { FieldGrid } from '~/components/alerts/field-grid';
import { ForField } from '~/components/alerts/for-field';
import { LogConditionFields } from '~/components/alerts/log-condition-fields';
import { UnitField } from '~/components/alerts/unit-field';
import { FieldRow } from '~/components/ui/field-row';
import { Input } from '~/components/ui/input';
import { getUnit, hasSubjectName, hasThreshold } from '~/utils/alerts';
import { isLimitValid, isOptionalNumber } from '~/utils/rule-form';
import type { RuleFormChange, RuleFormState } from '~/utils/rule-form';

type ConditionFieldsProps = {
  form: RuleFormState;
  onChange: RuleFormChange;
};

export function ConditionFields(props: ConditionFieldsProps) {
  const { form, onChange } = props;

  const { kind } = form;
  const forField = <ForField kind={kind} value={form.forSecs} onValueChange={(forSecs) => onChange({ forSecs })} />;

  if (kind === 'logs') {
    return <LogConditionFields form={form} onChange={onChange} />;
  }

  if (hasThreshold(kind)) {
    const direction = kind === 'cert_days' ? 'Below' : 'Above';
    const unit = getUnit(kind);

    return (
      <FieldGrid columns={form.isWarningOpen ? 3 : 2}>
        <UnitField
          label={form.isWarningOpen ? `Critical ${direction}` : direction}
          unit={unit}
          value={form.critical}
          onValueChange={(critical) => onChange({ critical })}
          isInvalid={!isLimitValid(form)}
        />
        {form.isWarningOpen && (
          <UnitField
            label={`Warning ${direction}`}
            unit={unit}
            value={form.warning}
            onValueChange={(warning) => onChange({ warning })}
            placeholder="Off"
            isInvalid={!isOptionalNumber(form.warning)}
          />
        )}
        {forField}
      </FieldGrid>
    );
  }

  if (hasSubjectName(kind)) {
    const isProcess = kind === 'process_stopped';

    return (
      <FieldGrid>
        <FieldRow label={isProcess ? 'Process' : 'Port'}>
          {({ inputId }) => (
            <Input
              id={inputId}
              value={form.subjectName}
              onValueChange={(subjectName) => onChange({ subjectName })}
              placeholder={isProcess ? 'Any watched process' : 'Any watched port'}
            />
          )}
        </FieldRow>
        {forField}
      </FieldGrid>
    );
  }

  return forField;
}
