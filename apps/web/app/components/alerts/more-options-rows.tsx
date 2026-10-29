import { FieldGrid } from '~/components/alerts/field-grid';
import { PlainSelect } from '~/components/alerts/plain-select';
import { DEFAULT_QUIET_WINDOW, QuietHoursField } from '~/components/alerts/quiet-hours-field';
import { SelectField } from '~/components/alerts/select-field';
import { SettingRow } from '~/components/alerts/setting-row';
import { Button } from '~/components/ui/button';
import { Input } from '~/components/ui/input';
import { Switch } from '~/components/ui/switch';
import type { NoData } from '~/utils/alerts';
import { hasThreshold, MESSAGE_LIMIT_OPTIONS, NO_DATA_OPTIONS, REMIND_OPTIONS } from '~/utils/alerts';
import {
  DECLARE_SEVERITY_OPTIONS,
  DECLARE_SHOWN_OPTIONS,
  getResolveGroups,
  readRecovery,
  readTargetNoun,
} from '~/utils/rule-form';
import type { ChangedOptions, RuleFormChange, RuleFormState } from '~/utils/rule-form';

type MoreOptionsRowsProps = {
  form: RuleFormState;
  onChange: RuleFormChange;
  changed?: ChangedOptions;
};

export function MoreOptionsRows(props: MoreOptionsRowsProps) {
  const { form, onChange, changed } = props;

  const { kind, quietHours } = form;
  const recovery = readRecovery(form);
  const resolveValue = `${recovery ?? 'limit'}:${form.resolveForSecs}`;

  const changeResolve = (value: string | null) => {
    const [nextRecovery, nextSecs] = (value ?? 'limit:0').split(':');
    onChange({ recovery: nextRecovery === 'limit' ? '' : (nextRecovery ?? ''), resolveForSecs: Number(nextSecs) });
  };

  return (
    <>
      <SettingRow
        title="Resolve When"
        description={
          hasThreshold(kind)
            ? `How long it must stay ${kind === 'cert_days' ? 'over' : 'under'} the limit before it resolves`
            : 'How long it must stay healthy before it resolves'
        }
        isChanged={changed?.resolve}
        control={(labelId) => (
          <PlainSelect labelId={labelId} value={resolveValue} onValueChange={changeResolve} groups={getResolveGroups(form)} />
        )}
      />
      {readTargetNoun(kind) !== 'cron' && (
        <SettingRow
          title="If Data Stops"
          description="When an agent disconnects or a check is paused"
          isChanged={changed?.noData}
          control={(labelId) => (
            <PlainSelect
              labelId={labelId}
              value={form.noData}
              onValueChange={(value) => onChange({ noData: (value ?? 'keep') as NoData })}
              items={NO_DATA_OPTIONS}
            />
          )}
        />
      )}
      <SettingRow
        title="Remind"
        description="While it is still firing"
        isChanged={changed?.remind}
        control={(labelId) => (
          <PlainSelect
            labelId={labelId}
            value={form.remind}
            onValueChange={(value) => onChange({ remind: value ?? 'off' })}
            items={REMIND_OPTIONS}
          />
        )}
      />
      <SettingRow
        title="Message Limit"
        description="Past it, one summary goes out and the rule stays quiet for the hour"
        isChanged={changed?.messageLimit}
        control={(labelId) => (
          <PlainSelect
            labelId={labelId}
            value={form.messageLimit}
            onValueChange={(value) => onChange({ messageLimit: value ?? 'off' })}
            items={MESSAGE_LIMIT_OPTIONS}
          />
        )}
      />
      <SettingRow
        title="Quiet Hours"
        description="No messages during planned work, like a nightly backup"
        isChanged={changed?.quietHours}
        actions={
          quietHours.length === 0 ? (
            <Button type="button" variant="tertiary" className="w-fit" onClick={() => onChange({ quietHours: [DEFAULT_QUIET_WINDOW] })}>
              Add Quiet Hours
            </Button>
          ) : (
            <div className="flex gap-2">
              {quietHours.length < 5 && (
                <Button
                  type="button"
                  variant="tertiary"
                  className="w-fit"
                  onClick={() => onChange({ quietHours: [...quietHours, DEFAULT_QUIET_WINDOW] })}
                >
                  Add Another Window
                </Button>
              )}
              <Button type="button" variant="tertiary" className="w-fit" onClick={() => onChange({ quietHours: [] })}>
                Remove Quiet Hours
              </Button>
            </div>
          )
        }
      >
        {quietHours.length > 0 && (
          <QuietHoursField
            windows={quietHours}
            timeZone={form.timeZone}
            onWindowsChange={(windows) => onChange({ quietHours: windows })}
            onTimeZoneChange={(timeZone) => onChange({ timeZone })}
          />
        )}
      </SettingRow>
      <SettingRow
        title="Open an Incident"
        description="For bad outages, so the team works from one timeline"
        isChanged={changed?.incident}
        actions={(labelId) => (
          <Switch.Root
            aria-labelledby={labelId}
            checked={form.declareIncident}
            onCheckedChange={(declareIncident) => onChange({ declareIncident })}
          >
            <Switch.Thumb />
          </Switch.Root>
        )}
      >
        {form.declareIncident && (
          <FieldGrid hint="Daroyan declares it, links this alert and adds what it fires on as an affected component.">
            <SelectField
              label="Severity"
              value={form.declareSeverity}
              onValueChange={(value) => onChange({ declareSeverity: value ?? 'major' })}
              items={DECLARE_SEVERITY_OPTIONS}
            />
            <SelectField
              label="Show on Status Pages"
              value={form.declareShown}
              onValueChange={(value) => onChange({ declareShown: value ?? 'shown' })}
              items={DECLARE_SHOWN_OPTIONS}
            />
          </FieldGrid>
        )}
      </SettingRow>
      <SettingRow
        title="Runbook URL"
        description="Linked in every message, so whoever is on call knows what to do"
        isChanged={changed?.runbook}
        control={(labelId) => (
          <Input
            aria-labelledby={labelId}
            type="url"
            value={form.runbookUrl}
            onValueChange={(runbookUrl) => onChange({ runbookUrl })}
            placeholder="https://wiki.example.com/runbooks/api"
          />
        )}
      />
    </>
  );
}
