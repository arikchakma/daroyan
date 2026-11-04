import { useQuery } from '@tanstack/react-query';
import { Link } from 'react-router';

import { Icon } from '~/components/icons/icon';
import { Button } from '~/components/ui/button';
import { DataTable } from '~/components/ui/data-table';
import { EmptyState, NoResults } from '~/components/ui/empty-state';
import { PageBar } from '~/components/ui/page-bar';
import { ViewsToolbar } from '~/components/ui/views-toolbar';
import { useSortParams } from '~/hooks/use-sort-params';
import { useStatusPagesTable } from '~/hooks/use-status-pages-table';
import { useIsAdmin } from '~/hooks/use-is-admin';
import { useViewParams } from '~/hooks/use-list-params';
import { statusPageQueries } from '~/queries/status-pages';

type VisibilityView = 'all' | 'public' | 'private';

const VIEWS: { value: VisibilityView; label: string }[] = [
  { value: 'all', label: 'All' },
  { value: 'public', label: 'Public' },
  { value: 'private', label: 'Private' },
];

const VIEW_VALUES = VIEWS.map((view) => view.value);

export function meta() {
  return [{ title: 'Status Pages · Daroyan' }];
}

export default function StatusPagesPage() {
  const isAdmin = useIsAdmin();
  const { data: pages, isLoading } = useQuery(statusPageQueries.list());
  const { search, view, setView, setSearch, reset, isFiltered } = useViewParams(VIEW_VALUES);

  const { sorting, onSortingChange } = useSortParams({
    sorts: { name: 'asc', components: 'desc', updated: 'desc' },
    defaultSort: 'name',
  });

  const term = search.trim().toLowerCase();
  const visible = (pages ?? []).filter(
    (page) =>
      (view === 'all' || page.is_public === (view === 'public')) &&
      (!term || page.name.toLowerCase().includes(term) || page.slug.includes(term))
  );

  const table = useStatusPagesTable({ pages: visible, sorting, onSortingChange });
  const isEmpty = pages?.length === 0;

  return (
    <div className="flex min-h-full flex-col bg-surface-default">
      <PageBar icon="globe" title="Status Pages">
        {!isEmpty && isAdmin && (
          <Button nativeButton={false} render={<Link to="/status-pages/new" />} className="w-fit">
            <Icon name="plus" />
            New Status Page
          </Button>
        )}
      </PageBar>

      {isEmpty ? (
        <EmptyState
          icon="globe"
          marks={['check', 'check', 'check']}
          title="Create your first status page"
          description="Pick the checks and crons to show. Your users get a public page to open when something looks wrong."
        >
          {isAdmin && (
            <Button nativeButton={false} render={<Link to="/status-pages/new" />} className="w-fit">
              <Icon name="plus" />
              New Status Page
            </Button>
          )}
        </EmptyState>
      ) : (
        <div className="flex flex-col gap-2 px-3 pt-1 pb-3">
          <ViewsToolbar
            views={VIEWS}
            view={view}
            onViewChange={setView}
            searchLabel="Search by name or slug"
            search={search}
            onSearchChange={setSearch}
            isViewChanged={isFiltered}
            onReset={reset}
          />
          <DataTable table={table} label="Status Pages" size="lg" isLoading={isLoading} />
          {pages && visible.length === 0 && (
            <NoResults title="No status pages found" onReset={reset} />
          )}
        </div>
      )}
    </div>
  );
}
