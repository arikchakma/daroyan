import { useMutation } from '@tanstack/react-query';
import { Link, useNavigate } from 'react-router';

import { Icon } from '~/components/icons/icon';
import { Button } from '~/components/ui/button';
import { Menu } from '~/components/ui/menu';
import { PageBar } from '~/components/ui/page-bar';
import { useCommandActions } from '~/hooks/use-command-actions';
import { useIsAdmin } from '~/hooks/use-is-admin';
import { queryClient } from '~/lib/query-client';
import { toast } from '~/lib/toast';
import { deleteServerOptions, renameServerOptions } from '~/queries/servers';

type TopBarProps = {
  serverId: string;
  serverName?: string;
};

export function TopBar(props: TopBarProps) {
  const { serverId, serverName } = props;

  const navigate = useNavigate();
  const isAdmin = useIsAdmin();
  const { mutate: deleteServer } = useMutation({
    ...deleteServerOptions(),
    meta: { errorMessage: 'The server could not be removed' },
  });

  const { mutate: renameServer } = useMutation({
    ...renameServerOptions(),
    meta: { errorMessage: 'The server could not be renamed' },
  });

  const rename = () => {
    const name = window.prompt('Server name', serverName ?? '')?.trim();
    if (!name || name === serverName) {
      return;
    }
    renameServer(
      { serverId, name },
      {
        onSuccess: () => {
          queryClient.invalidateQueries({ queryKey: ['servers'] });
          toast.success(`Renamed to ${name}`);
        },
      }
    );
  };

  const remove = () => {
    if (!window.confirm(`Remove ${serverName ?? 'this server'}? Its history is deleted too.`)) {
      return;
    }
    deleteServer(
      { serverId },
      {
        onSuccess: () => {
          queryClient.invalidateQueries({ queryKey: ['servers'] });
          toast.success(`Removed ${serverName ?? 'the server'}`);
          navigate('/servers');
        },
      }
    );
  };

  useCommandActions(serverName ?? 'Server', [
    {
      label: 'View Logs',
      icon: 'terminal',
      onSelect: () => void navigate(`/logs?server_id=${serverId}`),
    },
    isAdmin && !!serverName && { label: 'Rename Server', icon: 'pencil', onSelect: rename },
    isAdmin && !!serverName && { label: 'Remove Server', icon: 'trash', onSelect: remove },
  ]);

  return (
    <PageBar backTo="/servers" icon="server" title="Servers">
      {serverName && (
        <>
          {isAdmin && (
            <Menu.Root>
              <Menu.Trigger render={<Button variant="secondary" className="w-fit" />}>
                More Actions
                <Icon name="chevron-down" />
              </Menu.Trigger>
              <Menu.Portal>
                <Menu.Positioner align="end">
                  <Menu.Popup>
                    <Menu.Item onClick={rename}>
                      <Icon name="pencil" />
                      Rename Server
                    </Menu.Item>
                    <Menu.Item onClick={remove} className="text-tone-critical-fg">
                      <Icon name="trash" />
                      Remove Server
                    </Menu.Item>
                  </Menu.Popup>
                </Menu.Positioner>
              </Menu.Portal>
            </Menu.Root>
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
