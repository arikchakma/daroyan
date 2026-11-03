import { useMutation } from '@tanstack/react-query';
import { useState } from 'react';
import { Link, useNavigate } from 'react-router';

import { Icon } from '~/components/icons/icon';
import { Button } from '~/components/ui/button';
import { ConfirmDialog } from '~/components/ui/confirm-dialog';
import { Menu } from '~/components/ui/menu';
import { PageBar } from '~/components/ui/page-bar';
import { useCommandActions } from '~/hooks/use-command-actions';
import { useIsAdmin } from '~/hooks/use-is-admin';
import { queryClient } from '~/lib/query-client';
import { toast } from '~/lib/toast';
import { deleteServerOptions, serverQueries } from '~/queries/servers';

import { RenameServerDialog } from './rename-server-dialog';

type TopBarProps = {
  serverId: string;
  serverName?: string;
};

export function TopBar(props: TopBarProps) {
  const { serverId, serverName } = props;

  const navigate = useNavigate();
  const isAdmin = useIsAdmin();
  const [isRenameOpen, setIsRenameOpen] = useState(false);
  const [isRemoveOpen, setIsRemoveOpen] = useState(false);

  const { mutate: deleteServer, isPending: isRemoving } = useMutation({
    ...deleteServerOptions(),
    meta: { errorMessage: 'The server could not be removed' },
  });

  const remove = () =>
    deleteServer(
      { serverId },
      {
        onSuccess: () => {
          queryClient.invalidateQueries({ queryKey: serverQueries.all() });
          toast.success(`Removed ${serverName ?? 'the server'}`);
          setIsRemoveOpen(false);
          navigate('/servers');
        },
      }
    );

  useCommandActions(serverName ?? 'Server', [
    {
      label: 'View Logs',
      icon: 'terminal',
      onSelect: () => void navigate(`/logs?server_id=${serverId}`),
    },
    isAdmin && !!serverName && { label: 'Rename Server', icon: 'pencil', onSelect: () => setIsRenameOpen(true) },
    isAdmin && !!serverName && { label: 'Remove Server', icon: 'trash', onSelect: () => setIsRemoveOpen(true) },
  ]);

  return (
    <PageBar backTo="/servers" icon="server" title="Servers">
      {serverName && (
        <>
          {isAdmin && (
            <>
              <Menu.Root>
                <Menu.Trigger render={<Button variant="secondary" className="w-fit" />}>
                  More Actions
                  <Icon name="chevron-down" />
                </Menu.Trigger>
                <Menu.Portal>
                  <Menu.Positioner align="end">
                    <Menu.Popup>
                      <Menu.Item onClick={() => setIsRenameOpen(true)}>
                        <Icon name="pencil" />
                        Rename Server
                      </Menu.Item>
                      <Menu.Item onClick={() => setIsRemoveOpen(true)} className="text-tone-critical-fg">
                        <Icon name="trash" />
                        Remove Server
                      </Menu.Item>
                    </Menu.Popup>
                  </Menu.Positioner>
                </Menu.Portal>
              </Menu.Root>

              <RenameServerDialog
                open={isRenameOpen}
                onOpenChange={setIsRenameOpen}
                serverId={serverId}
                serverName={serverName}
              />
              <ConfirmDialog
                open={isRemoveOpen}
                onOpenChange={setIsRemoveOpen}
                title={`Remove ${serverName}?`}
                description="Its history is deleted too."
                confirmLabel="Remove Server"
                isPending={isRemoving}
                onConfirm={remove}
              />
            </>
          )}
          <Button nativeButton={false} render={<Link to={`/logs?server_id=${serverId}`} />} className="w-fit">
            <Icon name="terminal" />
            View Logs
          </Button>
        </>
      )}
    </PageBar>
  );
}
