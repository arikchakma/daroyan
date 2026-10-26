import { useQuery } from '@tanstack/react-query';
import { createColumnHelper } from '@tanstack/react-table';
import { useMemo } from 'react';
import { Link, useSearchParams } from 'react-router';

import { Button } from '~/components/ui/button';
import { DataTable } from '~/components/ui/data-table';
import { EmptyState, NoResults } from '~/components/ui/empty-state';
import { PageBar } from '~/components/ui/page-bar';
import { ViewsToolbar } from '~/components/ui/views-toolbar';
import { useDataTable } from '~/hooks/use-data-table';
import { ServiceDot } from '~/hooks/use-traces-table';
import type { DataTableFeatures } from '~/lib/table';
import { listIssuesOptions } from '~/queries/traces';
import { formatCount, formatRelative } from '~/utils/format';
import type { Issue } from '~/utils/traces';

type IssueRow = Issue & { id: string };

const helper = createColumnHelper<DataTableFeatures, IssueRow>();

const columns = helper.columns([
  helper.display({
    id: 'issue',
    header: 'Issue',
    meta: { skeleton: 'lines' },
    cell: ({ row }) => {
      const issue = row.original;
      return (
        <span className="flex min-w-0 flex-col gap-0.5">
          <Link
            to={`/issues/${issue.fingerprint}`}
            className="flex min-w-0 items-baseline gap-2 outline-none after:absolute after:inset-0 focus-visible:underline"
          >
            <span className="shrink-0 font-mono text-code text-tone-critical-fg">{issue.kind}</span>
            <span className="truncate text-body text-text-primary">{issue.message}</span>
          </Link>
          <span className="flex min-w-0 items-center gap-1.5 font-mono text-code-small text-text-tertiary">
            <ServiceDot service={issue.service} />
            <span className="truncate">{issue.service}</span>
          </span>
        </span>
      );
    },
  }),
  helper.accessor('count', {
    header: 'Events',
    meta: { width: 110, align: 'right' },
    cell: ({ getValue }) => formatCount(getValue()),
  }),
  helper.accessor('first_seen_at', {
    header: 'First Seen',
    meta: { width: 130, align: 'right', hideBelow: 'md' },
    cell: ({ getValue }) => formatRelative(getValue() / 1_000),
  }),
  helper.accessor('last_seen_at', {
    header: 'Last Seen',
    meta: { width: 130, align: 'right' },
    cell: ({ getValue }) => formatRelative(getValue() / 1_000),
  }),
]);

export function meta() {
  return [{ title: 'Issues · Daroyan' }];
}

export default function IssuesPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const service = searchParams.get('service') ?? '';
  const search = searchParams.get('q') ?? '';

  const { data: issues = [], isLoading } = useQuery(listIssuesOptions());
  const services = useMemo(() => [...new Set(issues.map((issue) => issue.service))].sort(), [issues]);
  const data = useMemo(() => {
    const needle = search.trim().toLowerCase();
    return issues
      .filter((issue) => !service || issue.service === service)
      .filter((issue) => !needle || `${issue.kind} ${issue.message}`.toLowerCase().includes(needle))
      .map((issue) => ({ ...issue, id: issue.fingerprint }));
  }, [issues, service, search]);
  const table = useDataTable({ columns, data });

  const setParam = (key: string, value: string | null) =>
    setSearchParams(
      (params) => {
        if (value) {
          params.set(key, value);
        } else {
          params.delete(key);
        }
        return params;
      },
      { replace: true }
    );

  if (!isLoading && issues.length === 0) {
    return (
      <div className="flex min-h-full flex-col bg-surface-default">
        <PageBar icon="triangle-alert" title="Issues" />
        <EmptyState
          icon="triangle-alert"
          marks={['circle-x', 'circle-x', 'check']}
          title="No issues yet"
          description="Errors your services send over OpenTelemetry are grouped here, with the stack trace and how often each happens."
        >
          <Button nativeButton={false} variant="secondary" render={<Link to="/traces/setup" />} className="w-fit">
            Set Up Traces
          </Button>
        </EmptyState>
      </div>
    );
  }

  return (
    <div className="flex min-h-full flex-col bg-surface-default">
      <PageBar icon="triangle-alert" title="Issues" />

      <div className="flex flex-col gap-2 px-3 pt-1 pb-3">
        <ViewsToolbar
          views={[{ value: '', label: 'All Services' }, ...services.map((item) => ({ value: item, label: item }))]}
          view={service}
          onViewChange={(next) => setParam('service', next || null)}
          searchLabel="Search exception type or message"
          search={search}
          onSearchChange={(next) => setParam('q', next || null)}
          isViewChanged={!!service || !!search}
          onReset={() => setSearchParams({}, { replace: true })}
        />
        <DataTable table={table} label="Issues" size="lg" isLoading={isLoading} />
        {!isLoading && data.length === 0 && (
          <NoResults title="No issues found" onReset={() => setSearchParams({}, { replace: true })} />
        )}
      </div>
    </div>
  );
}
