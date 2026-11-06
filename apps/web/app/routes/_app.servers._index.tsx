import type { Schemas } from '@daroyan/client';
import { useQuery } from '@tanstack/react-query';
import { Link } from 'react-router';

import { Icon } from '~/components/icons/icon';
import { Button } from '~/components/ui/button';
import { DataTable } from '~/components/ui/data-table';
import { EmptyState, NoResults } from '~/components/ui/empty-state';
import { PageBar } from '~/components/ui/page-bar';
import { ViewsToolbar } from '~/components/ui/views-toolbar';
import { useServersTable } from '~/hooks/use-servers-table';
import { useSortParams } from '~/hooks/use-sort-params';
import { useIsAdmin } from '~/hooks/use-is-admin';
import { useViewParams } from '~/hooks/use-list-params';
import { serverQueries } from '~/queries/servers';

type StatusView = 'all' | Schemas['ServerStatus'];

const VIEWS: { value: StatusView; label: string }[] = [
  { value: 'all', label: 'All' },
  { value: 'healthy', label: 'Healthy' },
  { value: 'warning', label: 'Warning' },
  { value: 'down', label: 'Down' },
  { value: 'pending', label: 'Pending' },
];

const VIEW_VALUES = VIEWS.map((view) => view.value);

export default function ServersPage() {
  const isAdmin = useIsAdmin();
  const { data: servers, isLoading } = useQuery(serverQueries.list());
  const { search, view, setView, setSearch, reset, isFiltered } = useViewParams(VIEW_VALUES);

  const { sorting, onSortingChange } = useSortParams({
    sorts: { name: 'asc', cpu: 'desc', memory: 'desc', disk: 'desc', lastSeen: 'desc' },
    defaultSort: 'name',
  });

  const term = search.trim().toLowerCase();
  const visible = (servers ?? []).filter(
    (server) =>
      (view === 'all' || server.status === view) &&
      (!term ||
        server.name.toLowerCase().includes(term) ||
        server.private_ip?.includes(term) ||
        server.labels.some((label) => label.toLowerCase().includes(term)))
  );

  const table = useServersTable({ servers: visible, sorting, onSortingChange });
  const isEmpty = servers?.length === 0;

  return (
    <div className="flex min-h-full flex-col bg-surface-default">
      <PageBar icon="server" title="Servers">
        {!isEmpty && isAdmin && (
          <Button nativeButton={false} render={<Link to="/servers/new" />} className="w-fit">
            <Icon name="plus" />
            Add Server
          </Button>
        )}
      </PageBar>

      {isEmpty ? (
        <EmptyState
          icon="server"
          marks={['check', 'check', 'clock']}
          title="Add your first server"
          description="Install the agent on a server. It shows up here with CPU, memory, disk and processes after its first report."
        >
          {isAdmin && (
            <Button nativeButton={false} render={<Link to="/servers/new" />} className="w-fit">
              <Icon name="plus" />
              Add Server
            </Button>
          )}
        </EmptyState>
      ) : (
        <div className="flex flex-col gap-2 px-3 pt-1 pb-3">
          <ViewsToolbar
            views={VIEWS}
            view={view}
            onViewChange={setView}
            searchLabel="Search by name, IP or label"
            search={search}
            onSearchChange={setSearch}
            isViewChanged={isFiltered}
            onReset={reset}
          />
          <DataTable table={table} label="Servers" size="lg" isLoading={isLoading} />
          {servers && visible.length === 0 && (
            <NoResults title="No servers found" onReset={reset} />
          )}
        </div>
      )}
    </div>
  );
}
