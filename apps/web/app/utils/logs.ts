import { describeRequest } from '~/hooks/use-logs-table';
import type { LogRow } from '~/hooks/use-logs-table';
import { DAY_MS, formatBytes, formatClock, formatDate, HOUR_MS, pluralize } from '~/utils/format';
import { getTimeRanges } from '~/utils/time-range';
import type { TimeRange } from '~/utils/time-range';

export const OTLP_SERVER_ID = 'otlp';
export const OTLP_SERVER_NAME = 'OpenTelemetry';

type LogOrigin = {
  service: string;
  stream: string;
  server_id: string;
  serverName: string;
};

export function getLogOrigin(line: LogOrigin) {
  if (line.server_id === OTLP_SERVER_ID) {
    return `${line.service} sent this over OpenTelemetry.`;
  }
  if (line.stream === 'otlp') {
    return `${line.service} on ${line.serverName} sent this over OpenTelemetry.`;
  }

  return `${line.service} wrote this to ${line.stream} on ${line.serverName}.`;
}

export function getLogRecord(line: LogRow) {
  return {
    time: new Date(line.at).toISOString(),
    server: line.serverName,
    source: line.service,
    stream: line.stream,
    level: line.level,
    message: line.message,
    attributes: line.attributes ?? {},
    ...(line.raw && { raw: line.raw }),
    ...(line.trace_id && { traceId: line.trace_id }),
  };
}

export function getLogAlertLink(line: LogRow) {
  const params = new URLSearchParams({ logService: line.service, logLevel: line.level === 'warn' ? 'warn' : 'error' });
  const text = line.message.split('\n')[0]?.trim() ?? '';
  if (!describeRequest(line) && text) {
    params.set('logText', text.slice(0, 120));
  }

  return `/alerts/new?${params}`;
}

const SECOND_CLOCK_FORMAT = new Intl.DateTimeFormat('en-GB', { hour: '2-digit', minute: '2-digit', second: '2-digit' });

function getBucketClock(at: number, bucketMs: number) {
  if (bucketMs < 60_000) {
    return SECOND_CLOCK_FORMAT.format(new Date(at));
  }

  return formatClock(at);
}

export function getBucketRange(at: number, bucketMs: number) {
  if (bucketMs >= DAY_MS) {
    return formatDate(at);
  }

  return `${getBucketClock(at, bucketMs)} – ${getBucketClock(at + bucketMs, bucketMs)}`;
}

export function getVolumeAxisLabel(at: number, bucketMs: number) {
  if (bucketMs >= HOUR_MS) {
    return formatDate(at);
  }

  return formatClock(at);
}

export function getZoomLabel(since: number, until: number) {
  const span = until - since;
  const isSameDay = new Date(since).toDateString() === new Date(until).toDateString();
  if (isSameDay) {
    return `${formatDate(since)}, ${getBucketClock(since, span)} – ${getBucketClock(until, span)}`;
  }

  return `${formatDate(since)} ${getBucketClock(since, span)} – ${formatDate(until)} ${getBucketClock(until, span)}`;
}

export function getPagingKey(params: URLSearchParams) {
  const rest = new URLSearchParams(params);
  rest.delete('page');

  return rest.toString();
}

export function getScanNote(scan: { files: number; bytes: number } | null | undefined) {
  if (!scan || scan.files === 0) {
    return undefined;
  }

  return `Read ${pluralize(scan.files, 'archived file')}, ${formatBytes(scan.bytes)}`;
}

export type LogRange = TimeRange;

export const DEFAULT_LOG_RANGE = '24h';

const HOT_RANGES = getTimeRanges(['15m', '1h', '24h', '7d']);

const ARCHIVE_DAYS = [30, 90];

function getDaysRange(days: number): LogRange {
  return { value: `${days}d`, label: `Last ${days} Days`, ms: days * DAY_MS };
}

export function getLogRanges(retentionDays: number | undefined) {
  if (!retentionDays) {
    return HOT_RANGES;
  }

  const days = [...ARCHIVE_DAYS.filter((day) => day < retentionDays), retentionDays];

  return [...HOT_RANGES, ...days.filter((day) => day > 7).map(getDaysRange)];
}

export function getLogRange(value: string | null) {
  const hot = HOT_RANGES.find((item) => item.value === value);
  if (hot) {
    return hot;
  }

  const days = Number(value?.match(/^(\d+)d$/)?.[1]);
  if (days > 0) {
    return getDaysRange(days);
  }

  return HOT_RANGES.find((item) => item.value === DEFAULT_LOG_RANGE)!;
}
