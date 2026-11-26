import { DAY_MS, HOUR_MS } from '~/utils/format';

export type TimeRange = {
  value: string;
  label: string;
  ms: number;
};

const TIME_RANGES: TimeRange[] = [
  { value: '15m', label: 'Last 15 Minutes', ms: 15 * 60_000 },
  { value: '1h', label: 'Last Hour', ms: HOUR_MS },
  { value: '6h', label: 'Last 6 Hours', ms: 6 * HOUR_MS },
  { value: '24h', label: 'Last 24 Hours', ms: DAY_MS },
  { value: '7d', label: 'Last 7 Days', ms: 7 * DAY_MS },
  { value: '30d', label: 'Last 30 Days', ms: 30 * DAY_MS },
];

export function getTimeRanges(values: string[]) {
  return TIME_RANGES.filter((range) => values.includes(range.value));
}

export function readTimeRange(ranges: TimeRange[], value: string | null) {
  return ranges.find((range) => range.value === value) ?? ranges[0]!;
}
