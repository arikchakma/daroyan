import type { Schemas } from '@daroyan/client';

import type { BadgeProps } from '~/components/ui/badge';

export type TraceSummary = Schemas['TraceSummary'];
export type TraceSpan = Schemas['TraceSpan'];
export type Issue = Schemas['Issue'];
export type IssueEvent = Schemas['IssueEvent'];

export const ERROR_COLOR = 'var(--color-tone-critical-fg)';
export const SLOW_MS = 10_000;

export function getServiceColor(service: string) {
  let hash = 0x811c9dc5;
  for (const char of service) {
    hash = Math.imul(hash ^ char.charCodeAt(0), 0x01000193) >>> 0;
  }
  const slot = hash % 20;
  return `oklch(${slot < 10 ? 0.68 : 0.52} 0.15 ${60 + (slot % 10) * 30})`;
}

export function formatMicros(us: number | null | undefined) {
  if (us === null || us === undefined) {
    return '—';
  }
  if (us < 1_000) {
    return `${Math.round(us)} µs`;
  }
  if (us < 1_000_000) {
    const ms = us / 1_000;
    return `${ms < 10 ? ms.toFixed(1).replace(/\.0$/, '') : Math.round(ms)} ms`;
  }
  const seconds = us / 1_000_000;
  return `${seconds < 10 ? seconds.toFixed(2).replace(/0$/, '').replace(/\.0$/, '') : seconds.toFixed(1)} s`;
}

const CLOCK = new Intl.DateTimeFormat('en-GB', { hour: '2-digit', minute: '2-digit', second: '2-digit' });

export function formatMicrosClock(us: number) {
  return CLOCK.format(new Date(us / 1_000));
}

type Tone = NonNullable<BadgeProps['tone']>;

type TraceStatus = { label: string; tone: Tone; isSlow: boolean };

export function getTraceStatus(
  trace: Pick<TraceSummary, 'http_status' | 'error_count' | 'duration_us'>
): TraceStatus {
  const status = trace.http_status;
  if (status !== null && status !== undefined && status >= 400) {
    return { label: String(status), tone: status >= 500 ? 'critical' : 'attention', isSlow: false };
  }
  if (trace.error_count > 0) {
    return { label: 'Error', tone: 'critical', isSlow: false };
  }
  if (trace.duration_us >= SLOW_MS * 1_000) {
    return { label: 'Slow', tone: 'attention', isSlow: true };
  }
  return { label: status ? String(status) : 'OK', tone: 'success', isSlow: false };
}

export function describeSpans(trace: Pick<TraceSummary, 'span_count' | 'services' | 'service'>) {
  const count = `${trace.span_count} ${trace.span_count === 1 ? 'span' : 'spans'}`;
  const services = [trace.service, ...trace.services.filter((service) => service !== trace.service)];
  if (services.length === 0) {
    return count;
  }
  if (services.length === 1) {
    return `${count} in ${services[0]}`;
  }
  return `${count} across ${services.slice(0, -1).join(', ')} and ${services.at(-1)}`;
}

export type SpanNode = TraceSpan & {
  depth: number;
  isLast: boolean;
  hasChildren: boolean;
  guides: boolean[];
  ancestors: string[];
};

export function orderSpans(spans: TraceSpan[]): SpanNode[] {
  const ids = new Set(spans.map((span) => span.span_id));
  const children = new Map<string, TraceSpan[]>();
  const roots: TraceSpan[] = [];
  for (const span of spans) {
    if (span.parent_span_id && ids.has(span.parent_span_id)) {
      children.set(span.parent_span_id, [...(children.get(span.parent_span_id) ?? []), span]);
    } else {
      roots.push(span);
    }
  }
  const byStart = (a: TraceSpan, b: TraceSpan) => a.started_at - b.started_at;
  const ordered: SpanNode[] = [];
  const visit = (span: TraceSpan, depth: number, isLast: boolean, guides: boolean[], ancestors: string[]) => {
    const kids = [...(children.get(span.span_id) ?? [])].sort(byStart);
    ordered.push({ ...span, depth, isLast, hasChildren: kids.length > 0, guides, ancestors });
    kids.forEach((child, index) =>
      visit(
        child,
        depth + 1,
        index === kids.length - 1,
        depth === 0 ? [] : [...guides, !isLast],
        [...ancestors, span.span_id]
      )
    );
  };
  roots.sort(byStart).forEach((root, index) => visit(root, 0, index === roots.length - 1, [], []));
  return ordered;
}


export function shortenId(id: string) {
  return id.length > 14 ? `${id.slice(0, 8)}…${id.slice(-6)}` : id;
}

export const TICKS = 4;

export function stringify(value: unknown) {
  return typeof value === 'string' ? value : JSON.stringify(value);
}
