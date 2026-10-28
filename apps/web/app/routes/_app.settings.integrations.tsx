import type { Schemas } from '@daroyan/client';
import { useMutation, useQuery } from '@tanstack/react-query';
import { useEffect, useState } from 'react';
import { useSearchParams } from 'react-router';

import { IntegrationDialog } from '~/components/alerts/integration-dialog';
import { Icon } from '~/components/icons/icon';
import { IntegrationRow } from '~/components/settings/integration-row';
import { SettingsBlock, SettingsCard, SettingsPage } from '~/components/settings/settings-page';
import { ConfirmDialog } from '~/components/ui/confirm-dialog';
import { useIsAdmin } from '~/hooks/use-is-admin';
import { queryClient } from '~/lib/query-client';
import { toast } from '~/lib/toast';
import { deleteIntegrationOptions, listIntegrationsOptions } from '~/queries/alerts';

export function meta() {
  return [{ title: 'Integrations · Daroyan' }];
}

export default function IntegrationsPage() {
  const isAdmin = useIsAdmin();
  const { data: integrations, isLoading } = useQuery(listIntegrationsOptions());
  const [editing, setEditing] = useState<Schemas['Integration']>();
  const [isDialogOpen, setIsDialogOpen] = useState(false);
  const [removing, setRemoving] = useState<Schemas['Integration']>();
  const [searchParams, setSearchParams] = useSearchParams();
  const connected = searchParams.get('connected');
  const connectError = searchParams.get('error');

  useEffect(() => {
    if (connected) {
      toast.success('Integration connected');
    } else if (connectError) {
      toast.error(connectError);
    }
    if (connected || connectError) {
      setSearchParams({}, { replace: true });
    }
  }, [connected, connectError, setSearchParams]);

  const remove = useMutation({
    ...deleteIntegrationOptions(),
    meta: { errorMessage: 'The integration could not be deleted' },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['integrations'] });
      toast.success(`Deleted ${removing?.name ?? 'the integration'}`);
      setRemoving(undefined);
    },
  });

  return (
    <SettingsPage title="Integrations" icon="bell">
      <SettingsBlock
        title="Channels"
        description="Alert rules send their messages here. Each rule picks which channels it uses."
      >
        <SettingsCard className="[&>*+*]:border-t [&>*+*]:border-border-row">
          {isLoading && <p className="px-4 py-3 text-body text-text-secondary">Loading…</p>}
          {integrations?.map((integration) => (
            <IntegrationRow
              key={integration.id}
              integration={integration}
              onEdit={() => {
                setEditing(integration);
                setIsDialogOpen(true);
              }}
              onDelete={() => setRemoving(integration)}
              canManage={isAdmin}
            />
          ))}
          {isAdmin && (
            <button
              type="button"
              onClick={() => {
                setEditing(undefined);
                setIsDialogOpen(true);
              }}
              className="flex items-center gap-2.5 bg-surface-subtle px-4 py-2.5 text-left text-body font-medium outline-none hover:bg-surface-fill focus-visible:ring-2 focus-visible:ring-text-link/40 focus-visible:ring-inset"
            >
              <Icon name="plus" />
              Add Integration
            </button>
          )}
        </SettingsCard>
      </SettingsBlock>

      <IntegrationDialog open={isDialogOpen} onOpenChange={setIsDialogOpen} integration={editing} />
      <ConfirmDialog
        open={!!removing}
        onOpenChange={(open) => !open && setRemoving(undefined)}
        title={`Delete ${removing?.name ?? 'Integration'}?`}
        description="Alert rules stop sending to it. Rules that only used this integration send nothing until you pick another."
        confirmLabel="Delete Integration"
        isPending={remove.isPending}
        onConfirm={() => removing && remove.mutate({ integrationId: removing.id })}
      />
    </SettingsPage>
  );
}
