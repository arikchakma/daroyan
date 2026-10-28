import type { Schemas } from '@daroyan/client';
import { useMutation } from '@tanstack/react-query';

import { getIntegrationLabel, IntegrationLogo } from '~/components/alerts/integration-logo';
import { Icon } from '~/components/icons/icon';
import { DeliveryStatus } from '~/components/settings/delivery-status';
import { Menu } from '~/components/ui/menu';
import { queryClient } from '~/lib/query-client';
import { toast } from '~/lib/toast';
import { testIntegrationOptions } from '~/queries/alerts';

type IntegrationRowProps = {
  integration: Schemas['Integration'];
  onEdit: () => void;
  onDelete: () => void;
  canManage: boolean;
};
export function IntegrationRow(props: IntegrationRowProps) {
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
