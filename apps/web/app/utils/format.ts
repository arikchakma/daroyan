const BYTE_UNITS = ['B', 'KB', 'MB', 'GB', 'TB', 'PB'];

function splitBytes(bytes: number) {
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < BYTE_UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return { value, unit: BYTE_UNITS[unit] };
}

function roundForDisplay(value: number) {
  return value >= 10 || Number.isInteger(value) ? Math.round(value).toString() : value.toFixed(1);
}

export function formatBytes(bytes: number) {
  const { value, unit } = splitBytes(bytes);
  return `${roundForDisplay(value)} ${unit}`;
}

export function formatSpeed(bytesPerSecond: number) {
  return `${formatBytes(bytesPerSecond)}/s`;
}

export function formatUsedOf(used: number, total: number) {
  const { unit } = splitBytes(total);
  const divisor = 1024 ** BYTE_UNITS.indexOf(unit);
  return `${roundForDisplay(used / divisor)} of ${roundForDisplay(total / divisor)} ${unit}`;
}

export function formatPercent(value: number, digits = 0) {
  return `${value.toFixed(digits)}%`;
}

export function getPercent(used: number, total: number) {
  return total > 0 ? (used / total) * 100 : 0;
}

export function formatCount(value: number) {
  return Math.round(value).toLocaleString('en-US');
}

export function formatUptimeDays(seconds: number) {
  const days = Math.floor(seconds / 86_400);
  if (days >= 1) {
    return `${days} ${days === 1 ? 'day' : 'days'}`;
  }
  return formatDuration(seconds);
}

export function formatDuration(seconds: number) {
  const days = Math.floor(seconds / 86_400);
  const hours = Math.floor((seconds % 86_400) / 3_600);
  const minutes = Math.floor((seconds % 3_600) / 60);
  if (days > 0) {
    return `${days}d ${hours}h`;
  }
  if (hours > 0) {
    return `${hours}h ${minutes}m`;
  }
  return `${Math.max(minutes, 0)}m`;
}

const RELATIVE_FORMAT = new Intl.RelativeTimeFormat('en', { numeric: 'always' });
const DATE_FORMAT = new Intl.DateTimeFormat('en-US', { month: 'short', day: 'numeric' });
const TIME_FORMAT = new Intl.DateTimeFormat('en-GB', { hour: '2-digit', minute: '2-digit' });

export function formatRelative(value: string | number, now = Date.now()) {
  const seconds = Math.round((now - new Date(value).getTime()) / 1000);
  if (seconds < 60) {
    return RELATIVE_FORMAT.format(-Math.max(seconds, 0), 'second');
  }
  if (seconds < 3_600) {
    return RELATIVE_FORMAT.format(-Math.floor(seconds / 60), 'minute');
  }
  if (seconds < 86_400) {
    return RELATIVE_FORMAT.format(-Math.floor(seconds / 3_600), 'hour');
  }
  return DATE_FORMAT.format(new Date(value));
}

export function formatDate(value: string | number) {
  return DATE_FORMAT.format(new Date(value));
}

export function formatClock(value: number) {
  return TIME_FORMAT.format(new Date(value));
}

export function formatEventTime(value: number, now = Date.now()) {
  const isToday = new Date(value).toDateString() === new Date(now).toDateString();
  return isToday ? formatClock(value) : formatDate(value);
}

const LOG_TIME_FORMAT = new Intl.DateTimeFormat('en-US', {
  hour: 'numeric',
  minute: '2-digit',
  second: '2-digit',
});

export function formatLogTime(value: number, now = Date.now()) {
  const isToday = new Date(value).toDateString() === new Date(now).toDateString();
  return isToday ? LOG_TIME_FORMAT.format(new Date(value)) : formatDate(value);
}

export function formatLogStamp(value: number) {
  return `${formatDate(value)} ${LOG_TIME_FORMAT.format(new Date(value))}`;
}

const LOG_CLOCK_FORMAT = new Intl.DateTimeFormat('en-GB', {
  hour: '2-digit',
  minute: '2-digit',
  second: '2-digit',
  fractionalSecondDigits: 3,
  hour12: false,
});

export function formatLogClock(value: number, now = Date.now()) {
  const clock = LOG_CLOCK_FORMAT.format(new Date(value));
  const isToday = new Date(value).toDateString() === new Date(now).toDateString();
  return isToday ? clock : `${formatDate(value)} ${clock}`;
}
