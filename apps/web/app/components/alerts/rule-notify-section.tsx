import type { Schemas } from '@daroyan/client';
import { useMutation, useQuery } from '@tanstack/react-query';
import { useState } from 'react';

import { FormSection } from '~/components/alerts/form-section';
import { IntegrationDialog } from '~/components/alerts/integration-dialog';
import { NotifyRow } from '~/components/alerts/notify-row';
import { Icon } from '~/components/icons/icon';
import { Button } from '~/components/ui/button';
import { queryClient } from '~/lib/query-client';
import { toast } from '~/lib/toast';
import { listIntegrationsOptions, testAlertRuleOptions } from '~/queries/alerts';
import type { NotifyLevel } from '~/utils/alerts';

type RuleNotifySectionProps = {
  levels: Record<string, NotifyLevel>;
  onLevelsChange: (levels: Record<string, NotifyLevel>) => void;
  testBody?: Schemas['TestAlertRuleBody'];
};

export function RuleNotifySection(props: RuleNotifySectionProps) {
  const { levels, onLevelsChange, testBody } = props;

  const [shownIds, setShownIds] = useState<string[]>([]);
  const [isAddOpen, setIsAddOpen] = useState(false);

  const { data: integrations = [] } = useQuery(listIntegrationsOptions());

  const test = useMutation({
    ...testAlertRuleOptions(),
    meta: { errorMessage: 'The test alert could not be sent' },
    onSuccess: () => toast.success('Test alert sent'),
    onSettled: () => queryClient.invalidateQueries({ queryKey: ['integrations'] }),
  });

  const visible = integrations.filter(
    (integration) => (levels[integration.id] ?? 'off') !== 'off' || shownIds.includes(integration.id)
  );
  const hiddenCount = integrations.length - visible.length;

  return (
    <>
      <FormSection
        title="Notify"
        description="Pick what each integration gets. Every one also hears when the alert resolves."
        action={
          <Button
            type="button"
            variant="tertiary"
            className="w-fit"
            disabled={!testBody || testBody.integration_ids.length === 0 || test.isPending}
            onClick={() => testBody && test.mutate(testBody)}
          >
            {test.isPending && <Icon name="loader" className="animate-spin" />}
            Send Test Alert
          </Button>
        }
      >
        {visible.map((integration) => (
          <NotifyRow
            key={integration.id}
            integration={integration}
            value={levels[integration.id] ?? 'off'}
            onValueChange={(value) => {
              setShownIds([...shownIds, integration.id]);
              onLevelsChange({ ...levels, [integration.id]: value });
            }}
          />
        ))}
        <button
          type="button"
          onClick={() => (hiddenCount > 0 ? setShownIds(integrations.map((integration) => integration.id)) : setIsAddOpen(true))}
          className="flex items-center gap-2 bg-surface-subtle px-3.5 py-2.5 text-left text-body font-medium outline-none hover:bg-surface-fill focus-visible:ring-2 focus-visible:ring-text-link/40 focus-visible:ring-inset"
        >
          <Icon name="plus" className="size-3.5" />
          Add Integration
        </button>
      </FormSection>
      <IntegrationDialog
        open={isAddOpen}
        onOpenChange={setIsAddOpen}
        onSaved={(saved) => {
          setShownIds([...shownIds, saved.id]);
          onLevelsChange({ ...levels, [saved.id]: 'both' });
        }}
      />
    </>
  );
}
