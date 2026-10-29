import { useState } from 'react';

import { FormSection } from '~/components/alerts/form-section';
import { MoreOptionsRows } from '~/components/alerts/more-options-rows';
import { Button } from '~/components/ui/button';
import { DEFAULT_MORE_OPTIONS, getChangedCountText, getChangedOptions, getMoreOptionsSummary } from '~/utils/rule-form';
import type { RuleFormChange, RuleFormState } from '~/utils/rule-form';

type RuleMoreOptionsProps = {
  form: RuleFormState;
  onChange: RuleFormChange;
  isEditing: boolean;
};

export function RuleMoreOptions(props: RuleMoreOptionsProps) {
  const { form, onChange, isEditing } = props;

  const changed = getChangedOptions(form);
  const changedCount = Object.values(changed).filter(Boolean).length;

  const [isOpen, setIsOpen] = useState(changedCount > 0);

  if (!isOpen) {
    return (
      <FormSection
        title="More Options"
        description="Resolving, reminders, quiet hours, incidents and a runbook link. The defaults suit most rules."
      >
        <div className="flex items-center gap-3 px-3.5 py-3">
          <div className="flex min-w-0 flex-1 flex-col gap-0.5">
            <span className="text-body font-medium">{changedCount === 0 ? 'Using Defaults' : 'Customized'}</span>
            <span className="text-body text-text-secondary">{getMoreOptionsSummary(form)}</span>
          </div>
          <Button type="button" variant="tertiary" className="w-fit" onClick={() => setIsOpen(true)}>
            Show
          </Button>
        </div>
      </FormSection>
    );
  }

  const marksChanges = isEditing && changedCount > 0;

  return (
    <FormSection
      title="More Options"
      description={
        marksChanges
          ? getChangedCountText(changedCount)
          : 'Resolving, reminders, quiet hours, incidents and a runbook link. The defaults suit most rules.'
      }
      action={
        <div className="flex gap-2">
          <Button
            type="button"
            variant="tertiary"
            className="w-fit"
            disabled={changedCount === 0}
            onClick={() => onChange(DEFAULT_MORE_OPTIONS)}
          >
            Reset to Defaults
          </Button>
          <Button type="button" variant="tertiary" className="w-fit" onClick={() => setIsOpen(false)}>
            Hide
          </Button>
        </div>
      }
    >
      <MoreOptionsRows form={form} onChange={onChange} changed={isEditing ? changed : undefined} />
    </FormSection>
  );
}
