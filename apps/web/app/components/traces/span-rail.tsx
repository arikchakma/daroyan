import type { ReactNode } from 'react';

import { cn } from '~/utils/classname';
import { formatMicros, stringify } from '~/utils/traces';
import type { SpanNode } from '~/utils/traces';

type RailSectionProps = {
  title: string;
  children: ReactNode;
};

function RailSection(props: RailSectionProps) {
  const { title, children } = props;

  return (
    <section className="flex flex-col gap-2 border-b border-border-row pb-4">
      <h2 className="truncate text-body font-medium">{title}</h2>
      {children}
    </section>
  );
}

type RailRowProps = {
  label: string;
  value: ReactNode;
  isText?: boolean;
};

function RailRow(props: RailRowProps) {
  const { label, value, isText } = props;

  return (
    <div className="flex items-start gap-3">
      <span className="min-w-0 flex-1 truncate font-mono text-code-small text-text-secondary">{label}</span>
      <span
        className={cn(
          'max-w-[60%] text-right break-all text-text-primary',
          isText ? 'text-body' : 'font-mono text-code-small'
        )}
      >
        {value}
      </span>
    </div>
  );
}

type SpanEvent = { name?: string; attributes?: Record<string, unknown> };
type SpanRailProps = {
  span: SpanNode;
};

export function SpanRail(props: SpanRailProps) {
  const { span } = props;

  const attributes = Object.entries((span.attributes ?? {}) as Record<string, unknown>);
  const resource = Object.entries((span.resource ?? {}) as Record<string, unknown>).filter(
    ([key]) => key !== 'service.name'
  );
  const exceptions = (Array.isArray(span.events) ? (span.events as SpanEvent[]) : []).filter(
    (event) => event.name === 'exception'
  );
  const isError = span.status === 'error';

  return (
    <aside aria-label="Span" className="flex w-65 max-w-full shrink-0 flex-col gap-4 pt-2 max-xl:w-200 xl:sticky xl:top-3 xl:h-fit">
      <RailSection title={span.name}>
        <RailRow label="service" value={span.service} isText />
        <RailRow label="duration" value={formatMicros(span.duration_us)} isText />
        {attributes.map(([key, value]) => (
          <RailRow key={key} label={key} value={stringify(value)} />
        ))}
        <RailRow
          label="status"
          value={<span className={cn(isError && 'text-tone-critical-fg')}>{isError ? 'Error' : 'OK'}</span>}
          isText
        />
        {span.status_message && <p className="text-body text-tone-critical-fg">{span.status_message}</p>}
      </RailSection>
      {exceptions.map((event, index) => (
        <RailSection key={index} title="Exception">
          <pre className="overflow-x-auto font-mono text-code-small whitespace-pre-wrap text-text-primary">
            <span className="text-tone-critical-fg">
              {stringify(event.attributes?.['exception.type'] ?? 'Error')}:{' '}
              {stringify(event.attributes?.['exception.message'] ?? '')}
            </span>
            {event.attributes?.['exception.stacktrace'] ? `\n${stringify(event.attributes['exception.stacktrace'])}` : ''}
          </pre>
        </RailSection>
      ))}
      {resource.length > 0 && (
        <RailSection title="Resource">
          {resource.map(([key, value]) => (
            <RailRow key={key} label={key} value={stringify(value)} />
          ))}
        </RailSection>
      )}
    </aside>
  );
}
