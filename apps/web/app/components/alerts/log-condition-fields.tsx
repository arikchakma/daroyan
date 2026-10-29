import { useQuery } from '@tanstack/react-query';

import { FieldGrid } from '~/components/alerts/field-grid';
import { ForField } from '~/components/alerts/for-field';
import { SelectField } from '~/components/alerts/select-field';
import { UnitField } from '~/components/alerts/unit-field';
import { FieldRow } from '~/components/ui/field-row';
import { Input } from '~/components/ui/input';
import { logSourcesOptions } from '~/queries/logs';
import { getUnit, LOG_LEVEL_OPTIONS, LOG_WINDOW_OPTIONS } from '~/utils/alerts';
import { formatSpan, isLimitValid } from '~/utils/rule-form';
import type { RuleFormChange, RuleFormState } from '~/utils/rule-form';

type LogConditionFieldsProps = {
  form: RuleFormState;
  onChange: RuleFormChange;
};

export function LogConditionFields(props: LogConditionFieldsProps) {
  const { form, onChange } = props;

  const { data: logSources } = useQuery(logSourcesOptions());

  const services = [...new Set((logSources ?? []).map((source) => source.service))].sort();
  const isValid = isLimitValid(form);

  return (
    <>
      <FieldGrid hint="Only processes with logs: true send lines.">
        <SelectField
          label="Level"
          value={form.logLevel}
          onValueChange={(value) => onChange({ logLevel: value ?? 'error' })}
          items={LOG_LEVEL_OPTIONS}
        />
        <SelectField
          label="Source"
          value={form.logService || 'any'}
          onValueChange={(value) => onChange({ logService: !value || value === 'any' ? '' : value })}
          items={[{ value: 'any', label: 'Any source' }, ...services.map((service) => ({ value: service, label: service }))]}
        />
      </FieldGrid>
      <FieldRow label="Text" hint="Optional. Matches lines that contain this text.">
        {({ inputId, hintId }) => (
          <Input
            id={inputId}
            aria-describedby={hintId}
            value={form.logText}
            onValueChange={(logText) => onChange({ logText })}
            placeholder="Any text"
            maxLength={200}
          />
        )}
      </FieldRow>
      <FieldGrid
        columns={3}
        hint={isValid ? `Fires when ${form.critical} or more matching lines arrive within ${formatSpan(form.logWithin)}.` : undefined}
      >
        <UnitField
          label="At Least"
          unit={getUnit(form.kind)}
          value={form.critical}
          onValueChange={(critical) => onChange({ critical })}
          isInvalid={!isValid}
        />
        <SelectField
          label="Within"
          value={form.logWithin.toString()}
          onValueChange={(value) => onChange({ logWithin: Number(value) })}
          items={LOG_WINDOW_OPTIONS.map((option) => ({ value: option.value.toString(), label: option.label }))}
        />
        <ForField kind={form.kind} value={form.forSecs} onValueChange={(forSecs) => onChange({ forSecs })} />
      </FieldGrid>
    </>
  );
}
