import type { Schemas } from '@daroyan/client';
import { useMutation } from '@tanstack/react-query';
import { useState } from 'react';
import { useNavigate } from 'react-router';

import { Button } from '~/components/ui/button';
import { ConfirmDialog } from '~/components/ui/confirm-dialog';
import { Switch } from '~/components/ui/switch';
import { useIsAdmin } from '~/hooks/use-is-admin';
import { queryClient } from '~/lib/query-client';
import { toast } from '~/lib/toast';
import { deleteAlertRuleOptions, updateAlertRuleOptions } from '~/queries/alerts';

type RuleStatusPanelProps = {
  rule: Schemas['AlertRule'];
};

export function RuleStatusPanel(props: RuleStatusPanelProps) {
  const { rule } = props;

  const navigate = useNavigate();
  const isAdmin = useIsAdmin();
  const [isDeleteOpen, setIsDeleteOpen] = useState(false);
  const toggle = useMutation({
    ...updateAlertRuleOptions(),
    meta: { errorMessage: 'The rule could not be updated' },
    onSuccess: (saved) => {
      queryClient.invalidateQueries({ queryKey: ['alert-rules'] });
      toast.success(saved.enabled ? 'Alerts are on' : 'Alerts are off');
    },
  });
  const remove = useMutation({
    ...deleteAlertRuleOptions(),
    meta: { errorMessage: 'The rule could not be deleted' },
    onSuccess: () => {
      navigate('/alerts');
      queryClient.invalidateQueries({ queryKey: ['alert-rules', 'list'] });
      queryClient.invalidateQueries({ queryKey: ['alerts'] });
      toast.success(`Deleted ${rule.name}`);
    },
  });

  return (
    <>
      <div className="h-px bg-border-row" />
      <div className="flex flex-col gap-2.5 text-body">
        <h2 className="font-medium">Status</h2>
        <div className="flex items-center gap-3">
          <span id="send-alerts" className="flex-1 text-text-secondary">
            Send alerts
          </span>
          <Switch.Root
            aria-labelledby="send-alerts"
            checked={rule.enabled}
            disabled={toggle.isPending}
            onCheckedChange={(enabled) => toggle.mutate({ ruleId: rule.id, body: { enabled } })}
          >
            <Switch.Thumb />
          </Switch.Root>
        </div>
        <p className="text-text-secondary">Turn off to pause the rule without losing it</p>
      </div>
      {isAdmin && (
        <>
          <div className="h-px bg-border-row" />
          <div className="flex flex-col gap-2.5">
            <h2 className="text-body font-medium">Delete Rule</h2>
            <Button type="button" variant="secondary" className="w-fit text-tone-critical-fg" onClick={() => setIsDeleteOpen(true)}>
              Delete Rule
            </Button>
          </div>
        </>
      )}
      <ConfirmDialog
        open={isDeleteOpen}
        onOpenChange={setIsDeleteOpen}
        title={`Delete ${rule.name}?`}
        description="Its history goes too. A firing alert stops without a resolve message."
        confirmLabel="Delete Rule"
        isPending={remove.isPending}
        onConfirm={() => remove.mutate({ ruleId: rule.id })}
      />
    </>
  );
}
