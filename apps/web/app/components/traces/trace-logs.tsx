import { useQuery } from '@tanstack/react-query';
import { Link } from 'react-router';

import { LevelBadge } from '~/hooks/use-logs-table';
import { logsOptions } from '~/queries/logs';
import { formatLogClock } from '~/utils/format';

const LOG_LIMIT = 50;

type TraceLogsProps = {
  traceId: string;
};

export function TraceLogs(props: TraceLogsProps) {
  const { traceId } = props;

  const query = `trace_id:${traceId}`;
  const { data } = useQuery(logsOptions({ q: query, offset: 0, limit: LOG_LIMIT }, false));
  const lines = [...(data?.lines ?? [])].reverse();

  return (
    <section className="flex flex-col gap-3">
      <div className="flex items-center gap-4">
        <h2 className="flex-1 text-title-section font-medium">Logs</h2>
        {data?.has_more && (
          <Link
            to={`/logs?range=7d&q=${encodeURIComponent(query)}`}
            className="rounded-xs text-body text-text-primary outline-none hover:underline focus-visible:underline"
          >
            View All Logs
          </Link>
        )}
      </div>
      <div className="overflow-hidden rounded-2xl border border-border-ring bg-surface-default shadow-card">
        {data && lines.length === 0 && (
          <p className="flex h-10 items-center px-4 text-body text-text-secondary">
            No log lines carry this trace ID.
          </p>
        )}
        <ul aria-label="Logs">
          {lines.map((line, index) => (
            <li
              key={`${line.at}-${index}`}
              className="flex min-h-10 items-start gap-3 border-b border-border-ring px-4 py-2.5 last:border-b-0"
            >
              <span className="shrink-0 font-mono text-code-small text-text-secondary tabular-nums">
                {formatLogClock(line.at)}
              </span>
              <LevelBadge level={line.level} />
              <span className="w-28 shrink-0 truncate text-caption text-text-secondary">{line.service}</span>
              <span className="min-w-0 flex-1 font-mono text-code-small wrap-anywhere text-text-primary">
                {line.message}
              </span>
            </li>
          ))}
        </ul>
      </div>
    </section>
  );
}
