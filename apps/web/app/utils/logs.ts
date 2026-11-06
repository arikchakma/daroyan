import { DAY_MS, formatClock, formatDate, HOUR_MS } from '~/utils/format';

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
