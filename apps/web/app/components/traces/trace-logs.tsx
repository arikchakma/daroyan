import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { Link } from 'react-router';

import { LogSheet } from '~/components/logs/log-sheet';
import { LevelBadge } from '~/hooks/use-logs-table';
import type { LogRow } from '~/hooks/use-logs-table';
import { logQueries } from '~/queries/logs';
import { serverQueries } from '~/queries/servers';
import { formatLogClock } from '~/utils/format';
import { OTLP_SERVER_ID, OTLP_SERVER_NAME } from '~/utils/logs';

const LOG_LIMIT = 50;
const WAIT_MS = 5_000;
const RECENT_MS = 2 * 60_000;
const SLACK_MS = 60 * 60_000;

type TraceLogsProps = {
  traceId: string;
  startedAt: number;
};

export function TraceLogs(props: TraceLogsProps) {
  const { traceId, startedAt } = props;

  const query = `trace_id:${traceId}`;
  const { data } = useQuery({
    ...logQueries.list({ q: query, since: startedAt - SLACK_MS, offset: 0, limit: LOG_LIMIT }, false),
    // Log batches arrive a few seconds after the spans, so keep looking while a
    // fresh trace has none yet.
    refetchInterval: (current) =>
      current.state.data?.lines.length === 0 && Date.now() - startedAt < RECENT_MS ? WAIT_MS : false,
  });
  const { data: servers } = useQuery(serverQueries.list());
  const [openId, setOpenId] = useState<string>();

  const serverNames = new Map([
    [OTLP_SERVER_ID, OTLP_SERVER_NAME],
    ...(servers ?? []).map((server) => [server.id, server.name] as const),
  ]);
  const lines: LogRow[] = [...(data?.lines ?? [])].reverse().map((line, index) => ({
    ...line,
    id: `${line.at}-${index}`,
    serverName: serverNames.get(line.server_id) ?? line.server_id,
    repeats: 1,
    repeatSpanMs: 0,
  }));
  const openIndex = lines.findIndex((line) => line.id === openId);

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
          {lines.map((line) => (
            <li
              key={line.id}
              className="relative flex h-10 items-center gap-3 border-b border-border-ring px-4 last:border-b-0 hover:bg-surface-row"
            >
              <span className="shrink-0 font-mono text-code-small text-text-secondary tabular-nums">
                {formatLogClock(line.at)}
              </span>
              <span className="w-14 shrink-0">
                <LevelBadge level={line.level} />
              </span>
              <span className="w-28 shrink-0 truncate text-caption text-text-secondary">{line.service}</span>
              <button
                type="button"
                onClick={() => setOpenId(line.id)}
                className="min-w-0 flex-1 cursor-pointer truncate text-left font-mono text-code-small text-text-primary outline-none after:absolute after:inset-0 focus-visible:underline"
              >
                {line.message.trimStart().split('\n')[0]}
              </button>
            </li>
          ))}
        </ul>
      </div>

      <LogSheet
        line={lines[openIndex]}
        canGoPrevious={openIndex > 0}
        canGoNext={openIndex >= 0 && openIndex < lines.length - 1}
        onNavigate={(step) => setOpenId(lines[openIndex + step]?.id)}
        onClose={() => setOpenId(undefined)}
      />
    </section>
  );
}
