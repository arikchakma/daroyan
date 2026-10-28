import type { Schemas } from '@daroyan/client';

import { cn } from '~/utils/classname';
import { formatEventTime } from '~/utils/format';

function getEventText(event: Schemas['EventItem']) {
  const data = event.data as Record<string, string | number | null | undefined>;
  switch (event.kind) {
    case 'agent_started':
      return { text: `Agent ${data.version ?? ''} started`, tone: 'default' as const };
    case 'agent_connected':
      return { text: 'Agent connected', tone: 'default' as const };
    case 'agent_disconnected':
      return { text: 'Agent disconnected', tone: 'critical' as const };
    case 'process_exited':
      return {
        text: data.code === null || data.code === undefined ? `${data.name} stopped` : `${data.name} exited ${data.code}`,
        tone: 'critical' as const,
      };
    case 'process_changed':
      return { text: `${data.name} restarted outside Daroyan (PID ${data.old_pid} → ${data.new_pid})`, tone: 'warning' as const };
    case 'process_restarted':
      return { text: `${data.name} restarted`, tone: 'warning' as const };
    case 'process_restart_failed':
      return { text: `${data.name} restart failed`, tone: 'critical' as const };
    case 'process_restart_gave_up':
      return { text: `${data.name} still down after ${data.attempts} restarts`, tone: 'critical' as const };
    default:
      return { text: event.kind, tone: 'default' as const };
  }
}

type EventRowProps = {
  event: Schemas['EventItem'];
};

export function EventRow(props: EventRowProps) {
  const { event } = props;

  const { text, tone } = getEventText(event);

  return (
    <li className="flex h-8.5 shrink-0 items-center gap-3 border-t border-border-row px-4 first:border-t-0">
      <span
        className={cn(
          'min-w-0 flex-1 truncate text-body text-text-primary',
          tone === 'critical' && 'text-tone-critical-fg',
          tone === 'warning' && 'text-tone-warning-fg'
        )}
      >
        {text}
      </span>
      <span className="shrink-0 font-mono text-code-small text-text-tertiary">{formatEventTime(event.at)}</span>
    </li>
  );
}
