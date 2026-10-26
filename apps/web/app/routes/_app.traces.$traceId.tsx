import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import type { ReactNode } from 'react';
import { Link, useParams } from 'react-router';

import { Icon } from '~/components/icons/icon';
import { Badge } from '~/components/ui/badge';
import { Button } from '~/components/ui/button';
import { Stat, StatStrip } from '~/components/ui/detail-rail';
import { PageBar } from '~/components/ui/page-bar';
import { useCopyToClipboard } from '~/hooks/use-copy-to-clipboard';
import { ServiceDot } from '~/hooks/use-traces-table';
import { toast } from '~/lib/toast';
import { traceOptions } from '~/queries/traces';
import { cn } from '~/utils/classname';
import { ERROR_COLOR, formatMicros, formatMicrosClock, getServiceColor, getTraceStatus, orderSpans } from '~/utils/traces';
import type { SpanNode } from '~/utils/traces';

const TICKS = 4;

export function meta() {
  return [{ title: 'Trace · Daroyan' }];
}

export default function TracePage() {
  const { traceId = '' } = useParams();
  const { data: spans, isError } = useQuery(traceOptions(traceId));

  if (isError || spans?.length === 0) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-1 pb-20">
        <p className="text-title-section font-medium text-text-primary">Trace Not Found</p>
        <p className="text-body text-text-secondary">It may be older than the retention window.</p>
      </div>
    );
  }

  if (!spans) {
    return null;
  }

  return <TraceDetail traceId={traceId} nodes={orderSpans(spans)} />;
}

function TraceDetail(props: { traceId: string; nodes: SpanNode[] }) {
  const { traceId, nodes } = props;

  const [copy] = useCopyToClipboard();
  const root = nodes[0]!;
  const [selectedId, setSelectedId] = useState(() => (nodes.find((node) => node.status === 'error') ?? root).span_id);
  const selected = nodes.find((node) => node.span_id === selectedId) ?? root;
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(new Set());
  const visible = nodes.filter((node) => !node.ancestors.some((id) => collapsed.has(id)));
  const traceStart = Math.min(...nodes.map((node) => node.started_at));
  const traceEnd = Math.max(...nodes.map((node) => node.started_at + node.duration_us));
  const total = Math.max(traceEnd - traceStart, 1);
  const services = [...new Set(nodes.map((node) => node.service))];
  const errorCount = nodes.filter((node) => node.status === 'error').length;
  const status = getTraceStatus({ http_status: root.http_status, error_count: errorCount, duration_us: root.duration_us });
  const host = readText(root.resource, 'host.name');

  return (
    <div className="flex min-h-full flex-col bg-surface-page">
      <PageBar backTo="/traces" icon="layers" title="Traces">
        <Button
          type="button"
          variant="secondary"
          className="w-fit"
          onClick={() => void copy(traceId).then((isCopied) => isCopied && toast.success('Copied the trace ID'))}
        >
          <Icon name="copy" />
          Copy Trace ID
        </Button>
        <Button
          nativeButton={false}
          render={<Link to={`/logs?range=7d&q=${encodeURIComponent(`trace_id:${traceId}`)}`} />}
          className="w-fit"
        >
          <Icon name="terminal" />
          View Logs
        </Button>
      </PageBar>

      <div className="flex justify-center gap-10 px-10 pt-3 pb-16 max-xl:flex-col max-xl:items-center max-lg:px-4">
        <div className="flex w-200 max-w-full min-w-0 flex-col gap-6">
          <div className="flex flex-col gap-1">
            <div className="flex min-w-0 items-center gap-2.5">
              <h1 className="min-w-0 truncate text-title-page font-semibold">{root.name}</h1>
              <Badge tone={status.tone}>
                <Icon name={status.tone === 'success' ? 'check' : 'circle-x'} />
                {status.tone === 'success' && root.http_status ? `${root.http_status} OK` : status.label}
              </Badge>
            </div>
            <p className="truncate font-mono text-code-small text-text-secondary">trace {traceId}</p>
          </div>

          <StatStrip columns={6}>
            <Stat label="Duration" value={formatMicros(root.duration_us)} />
            <Stat label="Spans" value={nodes.length.toString()} />
            <Stat label="Services" value={services.length.toString()} detail={services.join(', ')} />
            <Stat
              label="Errors"
              value={errorCount.toString()}
              valueClassName={errorCount > 0 ? 'text-tone-critical-fg' : undefined}
            />
            <Stat label="Server" value={host ?? '—'} />
            <Stat label="Started" value={formatMicrosClock(root.started_at)} />
          </StatStrip>

          <section className="flex flex-col gap-3">
            <div className="flex items-center gap-4">
              <h2 className="flex-1 text-title-section font-medium">Spans</h2>
              {services.map((service) => (
                <span key={service} className="flex items-center gap-1.25 text-[11px] leading-3.5 text-text-secondary">
                  <ServiceDot service={service} />
                  {service}
                </span>
              ))}
            </div>
            <div className="overflow-x-auto rounded-2xl border border-border-ring bg-surface-default pb-1 shadow-card">
              <div className="min-w-160">
                <div className="flex h-8 items-center bg-surface-row px-4">
                  <span className="w-75 shrink-0 text-caption font-medium text-text-secondary">Span</span>
                  <div className="relative h-3.5 flex-1 text-[11px] leading-3.5 text-text-tertiary">
                    {Array.from({ length: TICKS }, (_, index) => (
                      <span
                        key={index}
                        className="absolute top-0 whitespace-nowrap"
                        style={{
                          left: `${(index / (TICKS - 1)) * 100}%`,
                          transform: `translateX(-${(index / (TICKS - 1)) * 100}%)`,
                        }}
                      >
                        {index === 0 ? '0 ms' : formatMicros((total / (TICKS - 1)) * index)}
                      </span>
                    ))}
                  </div>
                </div>
                <ul aria-label="Spans">
                  {visible.map((node) => (
                    <WaterfallRow
                      key={node.span_id}
                      node={node}
                      isRoot={node === root}
                      isCollapsed={collapsed.has(node.span_id)}
                      onToggle={() =>
                        setCollapsed((current) => {
                          const next = new Set(current);
                          if (!next.delete(node.span_id)) {
                            next.add(node.span_id);
                          }
                          return next;
                        })
                      }
                      color={getServiceColor(node.service)}
                      offset={(node.started_at - traceStart) / total}
                      width={node.duration_us / total}
                      isSelected={node.span_id === selected.span_id}
                      onSelect={() => setSelectedId(node.span_id)}
                    />
                  ))}
                </ul>
              </div>
            </div>
          </section>
        </div>

        <SpanRail span={selected} />
      </div>
    </div>
  );
}

function readText(values: unknown, key: string) {
  const value = values && typeof values === 'object' ? (values as Record<string, unknown>)[key] : undefined;
  return value === undefined || value === null ? undefined : stringify(value);
}

function stringify(value: unknown) {
  return typeof value === 'string' ? value : JSON.stringify(value);
}

const INDENT = 16;
const GUIDE_X = 6;

type WaterfallRowProps = {
  node: SpanNode;
  isRoot: boolean;
  isCollapsed: boolean;
  onToggle: () => void;
  color: string;
  offset: number;
  width: number;
  isSelected: boolean;
  onSelect: () => void;
};

function WaterfallRow(props: WaterfallRowProps) {
  const { node, isRoot, isCollapsed, onToggle, color, offset, width, isSelected, onSelect } = props;

  const isError = node.status === 'error';
  const isLabelBefore = offset + width > 0.7;
  const ownX = (node.depth - 1) * INDENT + GUIDE_X;

  return (
    <li className="relative">
      <button
        type="button"
        aria-pressed={isSelected}
        onClick={onSelect}
        className="flex h-8.5 w-full items-stretch border-t border-border-row px-4 text-left outline-none hover:bg-surface-subtle focus-visible:bg-surface-subtle aria-pressed:bg-surface-subtle"
      >
        <span className="relative flex w-75 shrink-0 items-center overflow-hidden">
          {node.guides.map(
            (isShown, index) =>
              isShown && (
                <span
                  key={index}
                  aria-hidden="true"
                  className="absolute -top-px bottom-0 w-px bg-border-strong"
                  style={{ left: index * INDENT + GUIDE_X }}
                />
              )
          )}
          {node.depth > 0 && (
            <>
              <span
                aria-hidden="true"
                className={cn('absolute -top-px w-px bg-border-strong', node.isLast ? 'h-[calc(50%+1px)]' : 'bottom-0')}
                style={{ left: ownX }}
              />
              <span
                aria-hidden="true"
                className="absolute top-1/2 h-px bg-border-strong"
                style={{ left: ownX, width: INDENT + (node.hasChildren ? -GUIDE_X - 2 : GUIDE_X + 3) }}
              />
            </>
          )}
          {node.hasChildren && !isCollapsed && (
            <span
              aria-hidden="true"
              className="absolute top-[calc(50%+7px)] bottom-0 w-px bg-border-strong"
              style={{ left: node.depth * INDENT + GUIDE_X }}
            />
          )}
          <span
            className="flex min-w-0 items-center gap-2"
            style={{ paddingLeft: node.depth * INDENT + GUIDE_X * 2 + 6 }}
          >
            <ServiceDot service={node.service} color={color} />
            <span
              className={cn(
                'truncate font-mono',
                isRoot ? 'text-code font-medium' : 'text-code-small',
                isError ? 'text-tone-critical-fg' : 'text-text-primary'
              )}
            >
              {node.name}
            </span>
          </span>
        </span>
        <span className="relative flex-1">
          {Array.from({ length: TICKS }, (_, index) => (
            <span
              key={index}
              aria-hidden="true"
              className="absolute -top-px bottom-0 w-px bg-border-row"
              style={{ left: `${(index / (TICKS - 1)) * 100}%` }}
            />
          ))}
          <span
            className="absolute top-1/2 h-2 -translate-y-1/2 rounded-[2px]"
            style={{
              left: `${offset * 100}%`,
              width: `max(${width * 100}%, 2px)`,
              backgroundColor: isError ? ERROR_COLOR : color,
            }}
          />
          <span
            className="absolute top-1/2 -translate-y-1/2 font-mono text-code-small whitespace-nowrap text-text-tertiary"
            style={
              isLabelBefore
                ? { right: `calc(${(1 - offset) * 100}% + 6px)` }
                : { left: `calc(${(offset + width) * 100}% + 6px)` }
            }
          >
            {formatMicros(node.duration_us)}
          </span>
        </span>
      </button>
      {node.hasChildren && (
        <button
          type="button"
          aria-label={isCollapsed ? `Expand ${node.name}` : `Collapse ${node.name}`}
          aria-expanded={!isCollapsed}
          onClick={onToggle}
          className="absolute top-1/2 flex size-4 -translate-y-1/2 items-center justify-center rounded-xs text-text-tertiary outline-none hover:bg-surface-fill hover:text-text-primary focus-visible:ring-2 focus-visible:ring-text-link/40"
          style={{ left: 16 + node.depth * INDENT + GUIDE_X - 8 }}
        >
          <Icon name={isCollapsed ? 'chevron-right' : 'chevron-down'} className="size-3" />
        </button>
      )}
    </li>
  );
}

function RailSection(props: { title: string; children: ReactNode }) {
  const { title, children } = props;

  return (
    <section className="flex flex-col gap-2 border-b border-border-row pb-4">
      <h2 className="truncate text-body font-medium">{title}</h2>
      {children}
    </section>
  );
}

function RailRow(props: { label: string; value: ReactNode; isText?: boolean }) {
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

function SpanRail(props: { span: SpanNode }) {
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
