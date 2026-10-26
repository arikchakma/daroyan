import type { Schemas } from '@daroyan/client';
import { useMutation, useQuery } from '@tanstack/react-query';
import { useEffect, useState } from 'react';
import { useSearchParams } from 'react-router';

import { getIntegrationLabel, IntegrationLogo } from '~/components/alerts/integration-logo';
import { IntegrationDialog } from '~/components/alerts/integration-dialog';
import { Icon } from '~/components/icons/icon';
import { SettingsBlock, SettingsCard, SettingsPage } from '~/components/settings/settings-page';
import { Badge } from '~/components/ui/badge';
import { ConfirmDialog } from '~/components/ui/confirm-dialog';
import { Menu } from '~/components/ui/menu';
import { useIsAdmin } from '~/hooks/use-is-admin';
import { queryClient } from '~/lib/query-client';
import { toast } from '~/lib/toast';
import { deleteIntegrationOptions, listIntegrationsOptions, testIntegrationOptions } from '~/queries/alerts';
import { formatRelative } from '~/utils/format';

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

type IntegrationRowProps = {
  integration: Schemas['Integration'];
  onEdit: () => void;
  onDelete: () => void;
  canManage: boolean;
};

function IntegrationRow(props: IntegrationRowProps) {
  const { integration, onEdit, onDelete, canManage } = props;

  const test = useMutation({
    ...testIntegrationOptions(),
    meta: { isSilent: true },
    onSuccess: () => toast.success(`Test message sent to ${integration.name}`),
    onError: (error) => toast.error(error.message),
    onSettled: () => queryClient.invalidateQueries({ queryKey: ['integrations'] }),
  });

  return (
    <div className="flex items-center gap-3 py-2.5 pr-2.5 pl-4">
      <IntegrationLogo kind={integration.kind} />
      <div className="flex min-w-0 flex-1 flex-col gap-px text-body">
        <span className="truncate font-medium">{integration.name}</span>
        <span className="truncate text-text-secondary">
          {getIntegrationLabel(integration.kind)} · {integration.target}
        </span>
      </div>
      <DeliveryStatus integration={integration} />
      {canManage && (
        <Menu.Root>
          <Menu.Trigger
            aria-label={`Actions for ${integration.name}`}
            className="flex size-8 shrink-0 items-center justify-center rounded-full outline-none hover:bg-surface-fill focus-visible:ring-2 focus-visible:ring-text-link/40 data-popup-open:bg-surface-fill"
          >
            <Icon name="more" />
          </Menu.Trigger>
          <Menu.Portal>
            <Menu.Positioner align="end">
              <Menu.Popup className="min-w-40">
                <Menu.Item onClick={() => test.mutate({ integrationId: integration.id })}>
                  <Icon name="send" />
                  Send Test
                </Menu.Item>
                <Menu.Item onClick={onEdit}>
                  <Icon name="pencil" />
                  Edit
                </Menu.Item>
                <Menu.Separator />
                <Menu.Item onClick={onDelete} className="text-tone-critical-fg">
                  <Icon name="trash" />
                  Delete
                </Menu.Item>
              </Menu.Popup>
            </Menu.Positioner>
          </Menu.Portal>
        </Menu.Root>
      )}
    </div>
  );
}

type DeliveryStatusProps = {
  integration: Schemas['Integration'];
};

function DeliveryStatus(props: DeliveryStatusProps) {
  const { integration } = props;

  if (!integration.last_sent_at) {
    return <span className="shrink-0 text-body text-text-secondary">No deliveries yet</span>;
  }
  if (integration.last_status !== 'failed') {
    return (
      <span className="shrink-0 text-body text-text-secondary">
        Delivered {formatRelative(integration.last_sent_at)}
      </span>
    );
  }
  return (
    <div className="flex max-w-70 min-w-0 shrink flex-col items-end gap-0.5">
      <div className="flex items-center gap-1.5">
        <Badge tone="critical">
          <Icon name="triangle-alert" className="size-3.5" />
          Failed
        </Badge>
        <span className="text-body whitespace-nowrap text-text-secondary">{formatRelative(integration.last_sent_at)}</span>
      </div>
      {integration.last_error && (
        <span className="max-w-full truncate text-body text-tone-critical-fg" title={integration.last_error}>
          {integration.last_error}
        </span>
      )}
    </div>
  );
}
