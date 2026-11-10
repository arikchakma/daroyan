import type { Schemas } from '@daroyan/client';

import { Skeleton } from '~/components/ui/skeleton';
import { describeRequest, formatAttributes } from '~/hooks/use-logs-table';
import type { LogRow } from '~/hooks/use-logs-table';
import { cn } from '~/utils/classname';
import { formatLogClock } from '~/utils/format';

type LogContextListProps = {
  line: LogRow;
  context?: Schemas['LogLine'][];
  isLoading: boolean;
};

export function LogContextList(props: LogContextListProps) {
  const { line, context, isLoading } = props;

  return (
    <ol className="flex flex-col overflow-hidden rounded-xl border border-border-ring bg-surface-default py-1 shadow-card">
      {isLoading &&
        Array.from({ length: 8 }, (_, index) => (
          <li key={index} className="px-4 py-1.5">
            <Skeleton className={index % 2 ? 'w-3/4' : 'w-1/2'} />
          </li>
        ))}
      {context?.map((item, index) => {
        const isCurrent = item.at === line.at && item.message === line.message;

        return (
          <li
            key={`${item.at}-${index}`}
            className={cn(
              'relative flex gap-4 px-4 py-1 font-mono text-code-small',
              isCurrent && 'bg-surface-fill',
              item.level === 'error' && 'text-tone-critical-fg'
            )}
          >
            <span
              aria-hidden="true"
              className={cn(
                'absolute top-1 bottom-1 left-0 w-0.5 rounded-full',
                item.level === 'error' && 'bg-brand',
                item.level === 'warn' && 'bg-toast-warning-icon'
              )}
            />
            <span className="w-24 shrink-0 text-text-tertiary tabular-nums">{formatLogClock(item.at)}</span>
            <span className="min-w-0 wrap-anywhere whitespace-pre-wrap">
              {[describeRequest(item), item.message || formatAttributes(item.attributes)].filter(Boolean).join(' · ')}
            </span>
          </li>
        );
      })}
    </ol>
  );
}
