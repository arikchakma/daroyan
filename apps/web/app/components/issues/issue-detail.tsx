import { createColumnHelper } from '@tanstack/react-table';
import { useMemo } from 'react';
import { Link } from 'react-router';

import { Icon } from '~/components/icons/icon';
import { DetailCard, DetailEmpty, DetailSection } from '~/components/servers/detail-table';
import { Button } from '~/components/ui/button';
import { DataTable } from '~/components/ui/data-table';
import { Stat, StatStrip } from '~/components/ui/detail-rail';
import { PageBar } from '~/components/ui/page-bar';
import { useDataTable } from '~/hooks/use-data-table';
import { ServiceDot } from '~/hooks/use-traces-table';
import type { DataTableFeatures } from '~/lib/table';
import { formatClock, formatCount, formatDate, formatEventTime, formatRelative } from '~/utils/format';
import { formatMicrosClock, shortenId } from '~/utils/traces';
import type { Issue, IssueEvent } from '~/utils/traces';

const OCCURRENCE_LIMIT = 20;

type OccurrenceRow = IssueEvent & { id: string };

const helper = createColumnHelper<DataTableFeatures, OccurrenceRow>();

const columns = helper.columns([
  helper.accessor('occurred_at', {
    header: 'Time',
    enableSorting: false,
    meta: { width: 120 },
    cell: ({ getValue }) => <span className="font-mono text-code-small">{formatEventTime(getValue() / 1_000)}</span>,
  }),
  helper.accessor('message', {
    header: 'Message',
    enableSorting: false,
    cell: ({ getValue }) => <span className="block truncate text-text-secondary">{getValue()}</span>,
  }),
  helper.accessor('trace_id', {
    header: 'Trace',
    enableSorting: false,
    meta: { width: 160, align: 'right' },
    cell: ({ getValue }) => (
      <Link
        to={`/traces/${getValue()}`}
        className="font-mono text-code-small outline-none after:absolute after:inset-0 focus-visible:underline"
      >
        {shortenId(getValue())}
      </Link>
    ),
  }),
]);

function readTopFrame(stacktrace: string | null | undefined) {
  const lines = stacktrace?.split('\n').map((line) => line.trim()) ?? [];
  return lines.find((line) => line.startsWith('at ')) ?? [...lines].reverse().find((line) => line.startsWith('File ')) ?? null;
}

function splitStack(kind: string, event: IssueEvent | undefined) {
  const heading = `${kind}: ${event?.message ?? ''}`;
  const stack = event?.stacktrace ?? '';
  const [first = '', ...rest] = stack.split('\n');
  return first.trim() === heading.trim() ? [heading, rest.join('\n')] : [heading, stack];
}

type IssueDetailProps = {
  issue: Issue;
  events: IssueEvent[];
  isEventsLoading: boolean;
};

export function IssueDetail(props: IssueDetailProps) {
  const { issue, events, isEventsLoading } = props;

  const latest = events[0];
  const topFrame = readTopFrame(latest?.stacktrace);
  const [heading, stackBody] = splitStack(issue.kind, latest);
  const data = useMemo(
    () => events.slice(0, OCCURRENCE_LIMIT).map((event) => ({ ...event, id: `${event.span_id}-${event.occurred_at}` })),
    [events]
  );
  const table = useDataTable({ columns, data });
  const firstSeenMs = issue.first_seen_at / 1_000;
  const lastSeenMs = issue.last_seen_at / 1_000;

  return (
    <div className="flex min-h-full flex-col bg-surface-default">
      <PageBar backTo="/issues" icon="triangle-alert" title="Issues">
        <Button
          nativeButton={false}
          variant="secondary"
          render={<Link to={`/traces/${issue.trace_id}`} />}
          className="w-fit"
        >
          <Icon name="layers" />
          View Latest Trace
        </Button>
      </PageBar>

      <div className="flex justify-center px-10 pt-3 pb-16 max-lg:px-4">
        <div className="flex w-235 max-w-full min-w-0 flex-col gap-6">
          <div className="flex flex-col gap-1">
            <h1 className="text-title-page font-semibold break-words">
              {issue.kind}: {issue.message}
            </h1>
            <p className="flex min-w-0 items-center gap-1.5 text-body whitespace-nowrap text-text-secondary">
              <ServiceDot service={issue.service} />
              {issue.service}
              {topFrame && (
                <>
                  <span aria-hidden="true">·</span>
                  <span className="min-w-0 flex-1 truncate font-mono text-code-small">{topFrame}</span>
                </>
              )}
            </p>
          </div>

          <StatStrip columns={4}>
            <Stat label="Events" value={formatCount(issue.count)} detail="all time" />
            <Stat label="First Seen" value={formatDate(firstSeenMs)} detail={formatClock(firstSeenMs)} />
            <Stat label="Last Seen" value={formatRelative(lastSeenMs)} detail={formatClock(lastSeenMs)} />
            <Stat label="Service" value={issue.service} />
          </StatStrip>

          {latest && (
            <DetailSection
              title="Latest Stack Trace"
              summary={`${formatMicrosClock(latest.occurred_at)} · span ${latest.span_id}`}
            >
              <pre className="overflow-x-auto rounded-2xl bg-surface-row px-4 py-3.5 font-mono text-code-small leading-5 text-text-primary">
                <span className="text-tone-critical-fg">{heading}</span>
                {stackBody ? `\n${stackBody}` : ''}
              </pre>
            </DetailSection>
          )}

          <DetailSection title="Recent Occurrences" summary={`latest ${OCCURRENCE_LIMIT}`}>
            <DetailCard>
              {isEventsLoading || data.length > 0 ? (
                <DataTable
                  table={table}
                  label="Recent Occurrences"
                  variant="card"
                  size="sm"
                  isLoading={isEventsLoading}
                />
              ) : (
                <DetailEmpty>No occurrences in the retention window.</DetailEmpty>
              )}
            </DetailCard>
          </DetailSection>
        </div>
      </div>
    </div>
  );
}
