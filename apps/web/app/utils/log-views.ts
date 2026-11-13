import { getTermCondition, getTermValue, parseQuery } from '~/utils/log-search';

export const VIEW_PARAMS = [
  'level',
  'q',
  'range',
  'server_id',
  'server_op',
  'service',
  'service_op',
  'method',
  'method_op',
  'status',
  'status_op',
] as const;

export type SavedLogView = {
  id: string;
  name: string;
  params: Record<string, string>;
};

export type ViewChip = {
  condition: string;
  value: string;
};

type ViewFilter = {
  label: string;
  negate: boolean;
  values: string[];
};

type ViewChipsInput = {
  levelLabel?: string;
  filters: ViewFilter[];
  q: string;
  rangeLabel?: string;
};

export function getViewParams(searchParams: URLSearchParams) {
  const params: Record<string, string> = {};
  for (const key of VIEW_PARAMS) {
    const value = searchParams.get(key);
    if (value) {
      params[key] = value;
    }
  }

  return params;
}

export function isSameView(left: Record<string, string>, right: Record<string, string>) {
  const keys = Object.keys(left);
  if (keys.length !== Object.keys(right).length) {
    return false;
  }

  return keys.every((key) => left[key] === right[key]);
}

export function getViewChips(input: ViewChipsInput) {
  const { levelLabel, filters, q, rangeLabel } = input;

  const chips: ViewChip[] = [];
  if (levelLabel) {
    chips.push({ condition: 'Level is', value: levelLabel });
  }
  for (const filter of filters) {
    if (filter.values.length) {
      chips.push({
        condition: `${filter.label} ${filter.negate ? 'is not' : 'is'}`,
        value: filter.values.join(', '),
      });
    }
  }
  for (const term of parseQuery(q)) {
    chips.push({ condition: getTermCondition(term), value: getTermValue(term) });
  }
  if (rangeLabel) {
    chips.push({ condition: 'Range is', value: rangeLabel });
  }

  return chips;
}
