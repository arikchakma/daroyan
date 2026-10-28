import type { Schemas } from '@daroyan/client';
import { useState } from 'react';

import { Badge } from '~/components/ui/badge';
import { formatClock } from '~/utils/format';

const VISIBLE_EVENTS = 8;
const EVENT_TONES = { problem: 'critical', recovery: 'success', neutral: 'neutral' } as const;
const AXIS_EVERY = 12;
const MIN_SCALE = 3;

type EventsTileProps = {
  events?: Schemas['FleetEvents'];
};

export function EventsTile(props: EventsTileProps) {
  const { events } = props;

  const [isExpanded, setIsExpanded] = useState(false);

  const buckets = events?.buckets ?? [];
  const items = events?.events ?? [];
  const visible = isExpanded ? items : items.slice(0, VISIBLE_EVENTS);
  const hidden = items.length - visible.length;
  const top = Math.max(MIN_SCALE, ...buckets.map((bucket) => bucket.problems + bucket.recoveries));

  return (
    <section className="flex flex-col overflow-hidden rounded-2xl border border-border-ring bg-surface-default shadow-card">
      <div className="flex flex-col gap-3 p-4">
        <div className="flex items-center gap-2">
          <h2 className="flex-1 text-title-section font-medium">Events</h2>
          <span className="text-body text-text-secondary">Last 24 hours</span>
        </div>
        <p className="flex items-baseline gap-1.5 whitespace-nowrap">
          <span className="text-body font-semibold">
            {events?.problems ?? 0} {events?.problems === 1 ? 'problem' : 'problems'}
          </span>
          <span className="truncate text-caption font-normal text-text-tertiary">
            {events?.recoveries ?? 0} recovered · {events?.open ?? 0} still open
          </span>
        </p>
        <div className="flex flex-col gap-1.5">
          <div className="flex h-16 gap-0.75">
            {buckets.map((bucket) => (
              <span
                key={bucket.at}
                title={`${formatClock(bucket.at)}: ${bucket.problems} problems, ${bucket.recoveries} recovered`}
                className="flex min-w-0.5 flex-1 flex-col justify-end gap-px"
              >
                {bucket.problems + bucket.recoveries === 0 ? (
                  <span className="h-0.75 rounded-[1px] bg-surface-fill" />
                ) : (
                  <>
                    {bucket.recoveries > 0 && (
                      <span
                        className="rounded-[2px] bg-tone-success-solid"
                        style={{ height: `${(bucket.recoveries / top) * 100}%` }}
                      />
                    )}
                    {bucket.problems > 0 && (
                      <span
                        className="rounded-[2px] bg-brand"
                        style={{ height: `${(bucket.problems / top) * 100}%` }}
                      />
                    )}
                  </>
                )}
              </span>
            ))}
          </div>
          <div className="flex justify-between text-caption text-text-tertiary">
            {buckets
              .filter((_, index) => index % AXIS_EVERY === 0)
              .map((bucket) => (
                <span key={bucket.at}>{formatClock(bucket.at)}</span>
              ))}
            <span>Now</span>
          </div>
          <div className="flex items-center gap-3.5 text-caption text-text-tertiary">
            <span className="flex items-center gap-1.5">
              <span className="size-2 rounded-[2px] bg-brand" />
              Problem
            </span>
            <span className="flex items-center gap-1.5">
              <span className="size-2 rounded-[2px] bg-tone-success-solid" />
              Recovered
            </span>
          </div>
        </div>
      </div>
      <ul className="flex flex-col [&>*]:border-t [&>*]:border-border-row">
        {items.length === 0 ? (
          <li className="flex items-center gap-2.5 px-4 py-2.75">
            <Badge tone="success">Quiet</Badge>
            <span className="truncate text-body text-text-secondary">Nothing happened in the last 24 hours.</span>
          </li>
        ) : (
          visible.map((event) => (
            <li key={`${event.at}-${event.label}-${event.title}`} className="flex items-center gap-2.5 px-4 py-2.75">
              <Badge tone={EVENT_TONES[event.outcome]}>{event.label}</Badge>
              <span className="flex min-w-0 flex-1 items-baseline gap-2">
                <span className="truncate text-body">{event.title}</span>
                <span className="truncate text-caption font-normal text-text-tertiary">{event.detail}</span>
              </span>
              <span className="shrink-0 text-caption font-normal text-text-tertiary">{formatClock(event.at)}</span>
            </li>
          ))
        )}
        {hidden > 0 && (
          <li>
            <button
              type="button"
              onClick={() => setIsExpanded(true)}
              className="w-full px-4 py-2.75 text-left text-body outline-none hover:bg-surface-subtle focus-visible:bg-surface-subtle"
            >
              Show {hidden} more
            </button>
          </li>
        )}
      </ul>
    </section>
  );
}
