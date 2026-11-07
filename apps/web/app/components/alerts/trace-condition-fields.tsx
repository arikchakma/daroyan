import { useQuery } from '@tanstack/react-query';

import { FieldGrid } from '~/components/alerts/field-grid';
import { ForField } from '~/components/alerts/for-field';
import { SelectField } from '~/components/alerts/select-field';
import { UnitField } from '~/components/alerts/unit-field';
import { FieldRow } from '~/components/ui/field-row';
import { Input } from '~/components/ui/input';
import { traceQueries } from '~/queries/traces';
import {
  getUnit,
  PERCENTILE_OPTIONS,
  TRACE_GROUP_OPTIONS,
  TRACE_SPANS_OPTIONS,
  TRACE_WINDOW_OPTIONS,
} from '~/utils/alerts';
import { DAY_MS } from '~/utils/format';
import { getTraceLimitHint, isLimitValid, isOptionalNumber, readMinRequests } from '~/utils/rule-form';
import type { RuleFormChange, RuleFormState } from '~/utils/rule-form';

type TraceConditionFieldsProps = {
  form: RuleFormState;
  onChange: RuleFormChange;
};

export function TraceConditionFields(props: TraceConditionFieldsProps) {
  const { form, onChange } = props;

  const { data: traceServices } = useQuery(traceQueries.services(DAY_MS));

  const services = (traceServices ?? []).map((item) => item.service);
  const unit = getUnit(form.kind);
  const isLatency = form.kind === 'trace_latency';
  const isValid = isLimitValid(form) && readMinRequests(form) !== null;
  const limitColumns = (isLatency ? 1 : 0) + (form.isWarningOpen ? 2 : 1);

  return (
    <>
      <FieldGrid
        columns={3}
        hint="Incoming requests are server, consumer and root spans, so calls inside a request do not count twice."
      >
        <SelectField
          label="Service"
          value={form.traceService || 'any'}
          onValueChange={(value) => onChange({ traceService: !value || value === 'any' ? '' : value })}
          items={[{ value: 'any', label: 'Any service' }, ...services.map((service) => ({ value: service, label: service }))]}
        />
        <SelectField
          label="Count"
          value={form.traceSpans}
          onValueChange={(value) => onChange({ traceSpans: value === 'all' ? 'all' : 'entry' })}
          items={TRACE_SPANS_OPTIONS}
        />
        <SelectField
          label="Group By"
          value={form.traceGroupBy}
          onValueChange={(value) => onChange({ traceGroupBy: value === 'route' ? 'route' : 'service' })}
          items={TRACE_GROUP_OPTIONS}
        />
      </FieldGrid>
      <FieldRow label="Route" hint="Optional. Matches the HTTP route or span name exactly, like /api/search.">
        {({ inputId, hintId }) => (
          <Input
            id={inputId}
            aria-describedby={hintId}
            value={form.traceRoute}
            onValueChange={(traceRoute) => onChange({ traceRoute })}
            placeholder="Any route"
            maxLength={200}
          />
        )}
      </FieldRow>
      <FieldGrid columns={limitColumns === 3 ? 3 : 2}>
        {isLatency && (
          <SelectField
            label="Percentile"
            value={form.tracePercentile}
            onValueChange={(value) => onChange({ tracePercentile: value === 'p50' || value === 'p99' ? value : 'p95' })}
            items={PERCENTILE_OPTIONS}
          />
        )}
        <UnitField
          label={form.isWarningOpen ? 'Critical Above' : 'Above'}
          unit={unit}
          value={form.critical}
          onValueChange={(critical) => onChange({ critical })}
          isInvalid={!isLimitValid(form)}
        />
        {form.isWarningOpen && (
          <UnitField
            label="Warning Above"
            unit={unit}
            value={form.warning}
            onValueChange={(warning) => onChange({ warning })}
            placeholder="Off"
            isInvalid={!isOptionalNumber(form.warning)}
          />
        )}
      </FieldGrid>
      <FieldGrid columns={3} hint={isValid ? getTraceLimitHint(form) : undefined}>
        <SelectField
          label="Within"
          value={form.traceWithin.toString()}
          onValueChange={(value) => onChange({ traceWithin: Number(value) })}
          items={TRACE_WINDOW_OPTIONS.map((option) => ({ value: option.value.toString(), label: option.label }))}
        />
        <UnitField
          label="At Least"
          unit="requests"
          value={form.traceMinRequests}
          onValueChange={(traceMinRequests) => onChange({ traceMinRequests })}
          isInvalid={readMinRequests(form) === null}
        />
        <ForField kind={form.kind} value={form.forSecs} onValueChange={(forSecs) => onChange({ forSecs })} />
      </FieldGrid>
    </>
  );
}
