import type { Schemas } from '@daroyan/client';
import { useMutation } from '@tanstack/react-query';
import { createColumnHelper } from '@tanstack/react-table';
import { useMemo } from 'react';

import { Icon } from '~/components/icons/icon';
import { Badge } from '~/components/ui/badge';
import { Menu } from '~/components/ui/menu';
import { Tooltip } from '~/components/ui/tooltip';
import { useDataTable } from '~/hooks/use-data-table';
import { useIsAdmin } from '~/hooks/use-is-admin';
import { queryClient } from '~/lib/query-client';
import type { DataTableFeatures } from '~/lib/table';
import { toast } from '~/lib/toast';
import { runCommandOptions } from '~/queries/servers';
import type { CommandAction } from '~/queries/servers';
import { cn } from '~/utils/classname';
import { formatBytes, formatDuration, formatPercent, formatSpeed } from '~/utils/format';

type ProcessRow = Schemas['ProcessState'] & { id: string };

const helper = createColumnHelper<DataTableFeatures, ProcessRow>();

const ACTION_DONE: Record<CommandAction, string> = {
  restart_process: 'Restarted',
  start_process: 'Started',
  stop_process: 'Stopped',
};

function Muted() {
  return <span className="text-text-tertiary">—</span>;
}

type ProcessActionsProps = {
  serverId: string;
  process: ProcessRow;
};

function ProcessActions(props: ProcessActionsProps) {
  const { serverId, process } = props;

  const { mutate: runCommand, isPending } = useMutation({
    ...runCommandOptions(),
    meta: { errorMessage: `The command for ${process.name} failed` },
  });

  const isAdmin = useIsAdmin();

  const run = (action: CommandAction) =>
    runCommand(
      { serverId, action, name: process.name },
      {
        onSuccess: () => {
          toast.success(`${ACTION_DONE[action]} ${process.name}`);
          void queryClient.invalidateQueries({ queryKey: ['servers', serverId] });
        },
      }
    );

  if (!isAdmin) {
    return null;
  }

  return (
    <Menu.Root>
      <Menu.Trigger
        aria-label={`Actions for ${process.name}`}
        disabled={isPending}
        className={cn(
          'relative z-1 inline-flex size-6 items-center justify-center rounded-full opacity-0 outline-none group-hover:opacity-100 hover:bg-surface-fill focus-visible:opacity-100 focus-visible:ring-2 focus-visible:ring-text-link/40 data-popup-open:opacity-100 pointer-coarse:opacity-100',
          isPending && 'opacity-100'
        )}
      >
        <Icon name={isPending ? 'loader' : 'more'} className={cn(isPending && 'animate-spin')} />
      </Menu.Trigger>
      <Menu.Portal>
        <Menu.Positioner align="end" sideOffset={4}>
          <Menu.Popup>
            <Menu.Item onClick={() => run('restart_process')}>
              <Icon name="refresh" />
              Restart
            </Menu.Item>
            {process.status === 'running' ? (
              <Menu.Item onClick={() => run('stop_process')}>
                <Icon name="pause" />
                Stop
              </Menu.Item>
            ) : (
              <Menu.Item onClick={() => run('start_process')}>
                <Icon name="play" />
                Start
              </Menu.Item>
            )}
          </Menu.Popup>
        </Menu.Positioner>
      </Menu.Portal>
    </Menu.Root>
  );
}

type WatchOnlyHintProps = {
  name: string;
};

function WatchOnlyHint(props: WatchOnlyHintProps) {
  const { name } = props;

  return (
    <Tooltip.Root>
      <Tooltip.Trigger
        delay={200}
        aria-label={`Why ${name} has no actions`}
        className="relative z-1 inline-flex size-6 items-center justify-center rounded-full text-text-tertiary outline-none hover:bg-surface-fill focus-visible:ring-2 focus-visible:ring-text-link/40"
      >
        <Icon name="info" />
      </Tooltip.Trigger>
      <Tooltip.Portal>
        <Tooltip.Positioner side="left">
          <Tooltip.Popup className="max-w-64 text-left">
            Watch only. It is a process entry, or commands are off on this server. Watch it with systemd or docker to control it.
          </Tooltip.Popup>
        </Tooltip.Positioner>
      </Tooltip.Portal>
    </Tooltip.Root>
  );
}

type UseProcessesTableOptions = {
  serverId: string;
  processes: Schemas['ProcessState'][];
};

export function useProcessesTable(options: UseProcessesTableOptions) {
  const { serverId, processes } = options;

  const rows = useMemo(() => processes.map((process) => ({ ...process, id: process.name })), [processes]);

  const columns = useMemo(
    () =>
      helper.columns([
        helper.accessor('name', {
          header: 'Process',
          enableSorting: false,
          cell: ({ getValue }) => <span className="block truncate font-medium">{getValue()}</span>,
        }),
        helper.accessor('status', {
          header: 'Status',
          enableSorting: false,
          meta: { width: 110, skeleton: 'badge' },
          cell: ({ getValue }) =>
            getValue() === 'running' ? (
              <Badge tone="success">
                <Icon name="check" />
                Running
              </Badge>
            ) : (
              <Badge tone="critical">
                <Icon name="circle-x" />
                Stopped
              </Badge>
            ),
        }),
        helper.accessor('started_at', {
          header: 'Uptime',
          enableSorting: false,
          meta: { width: 84, align: 'right' },
          cell: ({ row }) =>
            row.original.status === 'running' && row.original.started_at ? (
              formatDuration((Date.now() - row.original.started_at) / 1000)
            ) : (
              <Muted />
            ),
        }),
        helper.accessor('cpu', {
          header: 'CPU',
          enableSorting: false,
          meta: { width: 68, align: 'right' },
          cell: ({ row }) => (row.original.status === 'running' ? formatPercent(row.original.cpu, 1) : <Muted />),
        }),
        helper.accessor('mem_used', {
          header: 'Memory',
          enableSorting: false,
          meta: { width: 80, align: 'right' },
          cell: ({ row }) => (row.original.status === 'running' ? formatBytes(row.original.mem_used) : <Muted />),
        }),
        helper.accessor('read_speed', {
          header: 'Read',
          enableSorting: false,
          meta: { width: 84, align: 'right', hideBelow: 'md' },
          cell: ({ getValue }) => (getValue() > 0 ? formatSpeed(getValue()) : <Muted />),
        }),
        helper.accessor('write_speed', {
          header: 'Write',
          enableSorting: false,
          meta: { width: 84, align: 'right', hideBelow: 'md' },
          cell: ({ getValue }) => (getValue() > 0 ? formatSpeed(getValue()) : <Muted />),
        }),
        helper.display({
          id: 'actions',
          enableHiding: false,
          meta: { width: 40, align: 'right', skeleton: 'none' },
          header: () => <span className="sr-only">Actions</span>,
          cell: ({ row }) =>
            row.original.controllable ? (
              <ProcessActions serverId={serverId} process={row.original} />
            ) : (
              <WatchOnlyHint name={row.original.name} />
            ),
        }),
      ]),
    [serverId]
  );

  return useDataTable({ columns, data: rows });
}
