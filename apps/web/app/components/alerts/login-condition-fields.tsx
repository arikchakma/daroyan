import { FieldGrid } from '~/components/alerts/field-grid';
import { SelectField } from '~/components/alerts/select-field';
import { UnitField } from '~/components/alerts/unit-field';
import { getUnit, LOG_WINDOW_OPTIONS } from '~/utils/alerts';
import { pluralize } from '~/utils/format';
import { formatSpan, isLimitValid } from '~/utils/rule-form';
import type { RuleFormChange, RuleFormState } from '~/utils/rule-form';

const ACCESS_HINT = 'It reads only servers that set agent.access_logs: true.';
const ROOT_HINT = `Fires when someone logs in over SSH as root, with one alert per address. ${ACCESS_HINT}`;

function getLimitHint(form: RuleFormState) {
  if (form.kind === 'login_new_ip') {
    return `Fires when someone logs in over SSH from an address that did not log in to the server in the last ${pluralize(Number(form.critical), 'day')}, with one alert per address. It learns for 24 hours after the first login on a server. ${ACCESS_HINT}`;
  }

  return `Fires when someone logs in over SSH after ${form.critical} or more failed attempts from the same address within ${formatSpan(form.loginWithin)}. ${ACCESS_HINT}`;
}

type LoginConditionFieldsProps = {
  form: RuleFormState;
  onChange: RuleFormChange;
};

export function LoginConditionFields(props: LoginConditionFieldsProps) {
  const { form, onChange } = props;

  const isValid = isLimitValid(form);

  if (form.kind === 'login_root') {
    return <p className="text-caption leading-4.5 text-text-secondary">{ROOT_HINT}</p>;
  }

  return (
    <FieldGrid hint={isValid ? getLimitHint(form) : undefined}>
      <UnitField
        label={form.kind === 'login_new_ip' ? 'Not Seen For' : 'At Least'}
        unit={getUnit(form.kind)}
        value={form.critical}
        onValueChange={(critical) => onChange({ critical })}
        isInvalid={!isValid}
      />
      {form.kind === 'login_after_failures' && (
        <SelectField
          label="Within"
          value={form.loginWithin.toString()}
          onValueChange={(value) => onChange({ loginWithin: Number(value) })}
          items={LOG_WINDOW_OPTIONS.map((option) => ({ value: option.value.toString(), label: option.label }))}
        />
      )}
    </FieldGrid>
  );
}
