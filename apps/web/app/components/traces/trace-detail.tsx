import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { Link } from 'react-router';

import { Icon } from '~/components/icons/icon';
import { SpanRail } from '~/components/traces/span-rail';
import { TraceLogs } from '~/components/traces/trace-logs';
import { WaterfallRow } from '~/components/traces/waterfall-row';
import { Badge } from '~/components/ui/badge';
import { Button } from '~/components/ui/button';
import { Stat, StatStrip } from '~/components/ui/detail-rail';
import { PageBar } from '~/components/ui/page-bar';
import { useCopyToClipboard } from '~/hooks/use-copy-to-clipboard';
import { ServiceDot } from '~/hooks/use-traces-table';
import { toast } from '~/lib/toast';
import { listServersOptions } from '~/queries/servers';
import { TICKS, formatMicros, formatMicrosClock, getServiceColor, getTraceStatus, stringify } from '~/utils/traces';
import type { SpanNode } from '~/utils/traces';

type TraceDetailProps = {
  traceId: string;
  nodes: SpanNode[];
};

export function TraceDetail(props: TraceDetailProps) {
  const { traceId, nodes } = props;

  const [copy] = useCopyToClipboard();
  const root = nodes[0]!;
  const [selectedId, setSelectedId] = useState(() => (nodes.find((node) => node.status === 'error') ?? root).span_id);
  const selected = nodes.find((node) => node.span_id === selectedId) ?? root;
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(new Set());
  const [timelineWidth, setTimelineWidth] = useState(0);
  const visible = nodes.filter((node) => !node.ancestors.some((id) => collapsed.has(id)));
  const traceStart = Math.min(...nodes.map((node) => node.started_at));
  const traceEnd = Math.max(...nodes.map((node) => node.started_at + node.duration_us));
  const total = Math.max(traceEnd - traceStart, 1);
  const services = [...new Set(nodes.map((node) => node.service))];
  const errorCount = nodes.filter((node) => node.status === 'error').length;
  const status = getTraceStatus({ http_status: root.http_status, error_count: errorCount, duration_us: root.duration_us });
  const host = readText(root.resource, 'host.name');
  const { data: servers } = useQuery(listServersOptions());
  const server = servers?.find((item) => item.id === root.server_id);

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
            <Stat label="Server" value={server?.name ?? host ?? '—'} />
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
                  <div
                    ref={(element) => {
                      if (!element) {
                        return;
                      }
                      const observer = new ResizeObserver(([entry]) => setTimelineWidth(entry!.contentRect.width));
                      observer.observe(element);
                      return () => observer.disconnect();
                    }}
                    className="relative h-3.5 flex-1 text-[11px] leading-3.5 text-text-tertiary"
                  >
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
                      timelineWidth={timelineWidth}
                      isSelected={node.span_id === selected.span_id}
                      onSelect={() => setSelectedId(node.span_id)}
                    />
                  ))}
                </ul>
              </div>
            </div>
          </section>

          <TraceLogs traceId={traceId} startedAt={root.started_at / 1_000} />
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
