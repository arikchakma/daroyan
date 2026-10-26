import type { Schemas } from '@daroyan/client';
import { createColumnHelper } from '@tanstack/react-table';
import type { ColumnVisibilityState } from '@tanstack/react-table';
import { useMemo } from 'react';

import { Badge } from '~/components/ui/badge';
import { useDataTable } from '~/hooks/use-data-table';
import type { DataTableFeatures } from '~/lib/table';
import { cn } from '~/utils/classname';
import { formatLogClock } from '~/utils/format';

export type LogRow = Schemas['LogLine'] & {
  id: string;
  serverName: string;
  repeats: number;
  repeatSpanMs: number;
};

export const HTTP_KEYS = ['http.method', 'url.path', 'http.status', 'duration_ms'];
const HIDDEN_KEYS = new Set([...HTTP_KEYS, 'stack.lines']);

export const COLUMN_LABELS: Record<string, string> = {
  status: 'Status',
  method: 'Method',
  path: 'Path',
  service: 'Source',
  duration: 'Duration',
};

const LEVEL_LABELS: Record<string, string> = { error: 'Error', warn: 'Warn', info: 'Info', debug: 'Debug' };

export function LevelBadge(props: { level: string }) {
  const { level } = props;

  const tone = level === 'error' ? 'critical' : level === 'warn' ? 'attention' : 'neutral';
  return <Badge tone={tone}>{LEVEL_LABELS[level] ?? level}</Badge>;
}

function readAttribute(line: Schemas['LogLine'], key: string) {
  return (line.attributes as Record<string, unknown> | null | undefined)?.[key];
}

export function hasHttp(line: Schemas['LogLine']) {
  return readAttribute(line, 'http.status') !== undefined;
}

export function describeRequest(line: Schemas['LogLine']) {
  const method = readAttribute(line, 'http.method');
  const path = readAttribute(line, 'url.path');
  const status = readAttribute(line, 'http.status');
  return method && path ? `${method} ${path}${status ? ` ${status}` : ''}` : '';
}

export function formatAttributes(attributes: Schemas['LogLine']['attributes']) {
  if (!attributes) {
    return '';
  }
  return Object.entries(attributes)
    .filter(([key]) => !HIDDEN_KEYS.has(key))
    .map(([key, value]) => `${key}=${typeof value === 'string' ? value : JSON.stringify(value)}`)
    .join(' ');
}

export function formatDuration(value: unknown) {
  if (typeof value !== 'number') {
    return '';
  }
  return value >= 1000 ? `${(value / 1000).toFixed(1)}s` : `${Math.round(value)}ms`;
}

function StatusChip(props: { status: unknown }) {
  const { status } = props;

  if (typeof status !== 'number') {
    return <span className="text-text-tertiary">—</span>;
  }
  return (
    <span
      className={cn(
        'inline-flex h-5 items-center rounded-xs px-1.5 font-mono text-caption tabular-nums',
        status >= 500 && 'bg-tone-critical-bg text-tone-critical-fg',
        status >= 400 && status < 500 && 'bg-tone-attention-bg text-tone-attention-fg',
        status < 400 && 'bg-surface-fill text-text-secondary'
      )}
    >
      {status}
    </span>
  );
}

const helper = createColumnHelper<DataTableFeatures, LogRow>();

type UseLogsTableOptions = {
  lines: LogRow[];
  isWrapped: boolean;
  columnVisibility: ColumnVisibilityState;
  onOpen: (line: LogRow) => void;
};

export function useLogsTable(options: UseLogsTableOptions) {
  const { lines, isWrapped, columnVisibility, onOpen } = options;

  const columns = useMemo(
    () =>
      helper.columns([
        helper.accessor('at', {
          header: 'Time',
          meta: { width: 132 },
          cell: ({ row }) => (
            <>
              <span
                aria-hidden="true"
                className={cn(
                  'absolute top-1.5 bottom-1.5 left-0 w-0.5 rounded-full',
                  row.original.level === 'error' && 'bg-brand',
                  row.original.level === 'warn' && 'bg-toast-warning-icon'
                )}
              />
              <span className="font-mono text-text-secondary tabular-nums">{formatLogClock(row.original.at)}</span>
            </>
          ),
        }),
        helper.accessor((line) => readAttribute(line, 'http.status'), {
          id: 'status',
          header: 'Status',
          meta: { width: 72, skeleton: 'badge', hideBelow: 'sm' },
          cell: ({ getValue }) => <StatusChip status={getValue()} />,
        }),
        helper.accessor((line) => readAttribute(line, 'http.method'), {
          id: 'method',
          header: 'Method',
          meta: { width: 76, hideBelow: 'md' },
          cell: ({ getValue }) => <span className="font-mono">{String(getValue() ?? '—')}</span>,
        }),
        helper.accessor((line) => readAttribute(line, 'url.path'), {
          id: 'path',
          header: 'Path',
          meta: { width: 220, hideBelow: 'md' },
          cell: ({ getValue }) => <span className="block truncate font-mono">{String(getValue() ?? '—')}</span>,
        }),
        helper.accessor('service', {
          header: 'Source',
          meta: { width: 120, hideBelow: 'lg' },
          cell: ({ getValue }) => <span className="block truncate text-text-secondary">{getValue()}</span>,
        }),
        helper.accessor('message', {
          header: 'Message',
          meta: { skeleton: 'text' },
          cell: ({ row }) => {
            const line = row.original;
            const attributes = formatAttributes(line.attributes);
            const stackLines = readAttribute(line, 'stack.lines');
            const firstLine = line.message.split('\n')[0];
            const repeat =
              line.repeats > 1 ? `×${line.repeats} in ${Math.max(Math.round(line.repeatSpanMs / 1000), 1)}s` : '';
            return (
              <button
                type="button"
                onClick={() => onOpen(line)}
                className={cn(
                  'block w-full text-left font-mono outline-none after:absolute after:inset-0 focus-visible:underline',
                  isWrapped ? 'break-all whitespace-pre-wrap' : 'truncate',
                  line.level === 'error' && 'text-tone-critical-fg'
                )}
              >
                {isWrapped ? line.message : firstLine || repeat}
                {typeof stackLines === 'number' && !isWrapped && (
                  <span className="ml-2 text-text-tertiary">+{stackLines} lines</span>
                )}
                {attributes && <span className="ml-2 text-text-tertiary">{attributes}</span>}
                {repeat && firstLine && <span className="ml-2 text-text-tertiary">{repeat}</span>}
              </button>
            );
          },
        }),
        helper.accessor((line) => readAttribute(line, 'duration_ms'), {
          id: 'duration',
          header: 'Duration',
          meta: { width: 84, align: 'right', hideBelow: 'sm' },
          cell: ({ getValue }) => (
            <span className="font-mono text-text-secondary tabular-nums">{formatDuration(getValue())}</span>
          ),
        }),
      ]),
    [isWrapped, onOpen]
  );

  return useDataTable({ data: lines, columns, state: { columnVisibility } });
}

export function collapseRepeats(lines: Schemas['LogLine'][], serverNames: Map<string, string>): LogRow[] {
  const rows: LogRow[] = [];
  for (const [index, line] of lines.entries()) {
    const previous = rows.at(-1);
    const isRepeat =
      previous &&
      previous.server_id === line.server_id &&
      previous.service === line.service &&
      previous.message === line.message &&
      readAttribute(previous, 'url.path') === readAttribute(line, 'url.path') &&
      readAttribute(previous, 'http.status') === readAttribute(line, 'http.status') &&
      formatAttributes(previous.attributes) === formatAttributes(line.attributes);
    if (previous && isRepeat) {
      previous.repeats += 1;
      previous.repeatSpanMs = previous.at - line.at;
      continue;
    }
    rows.push({
      ...line,
      id: `${line.at}-${index}`,
      serverName: serverNames.get(line.server_id) ?? line.server_id,
      repeats: 1,
      repeatSpanMs: 0,
    });
  }
  return rows;
}
