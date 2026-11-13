export const ANY_KEY = '*';

export type QueryTermKind = 'text' | 'regex' | 'list' | 'above' | 'below';

export type QueryTerm = {
  key: string | null;
  kind: QueryTermKind;
  values: string[];
  negate: boolean;
};

export type SearchOperator =
  | 'contains'
  | 'not_contains'
  | 'regex'
  | 'is'
  | 'is_not'
  | 'exists'
  | 'not_exists'
  | 'above'
  | 'below';

export const OPERATOR_LABELS: Record<SearchOperator, string> = {
  contains: 'contains',
  not_contains: 'does not contain',
  regex: 'matches regex',
  is: 'is',
  is_not: 'is not',
  exists: 'exists',
  not_exists: 'does not exist',
  above: 'is above',
  below: 'is below',
};

const TEXT_OPERATORS: SearchOperator[] = ['contains', 'not_contains', 'regex'];

const KEY_OPERATORS: SearchOperator[] = [
  'is',
  'is_not',
  'contains',
  'not_contains',
  'regex',
  'exists',
  'not_exists',
  'above',
  'below',
];

const KEY_LABELS: Record<string, string> = {
  service: 'Source',
  source: 'Source',
  trace_id: 'Trace',
  trace: 'Trace',
  method: 'Method',
  status: 'Status',
  path: 'Path',
};

type Token = {
  key: string | null;
  value: string;
  isQuoted: boolean;
  isRegex: boolean;
};

function isKeyChar(char: string) {
  return /[A-Za-z0-9._\-@*]/.test(char);
}

function isSpace(char: string | undefined) {
  return char !== undefined && /\s/.test(char);
}

function readKeyEnd(chars: string[], start: number) {
  let colon = start;
  while (colon < chars.length && isKeyChar(chars[colon]!)) {
    colon += 1;
  }
  if (chars[colon] !== ':') {
    return null;
  }

  const key = chars.slice(start, colon).join('').replace(/^@/, '');
  const isKey = key === ANY_KEY || (key.length > 0 && !key.includes('*'));
  const next = chars[colon + 1];
  const hasValue = next !== undefined && !isSpace(next);
  const isUrl = next === '/' && chars[colon + 2] === '/';

  return isKey && hasValue && !isUrl ? colon : null;
}

function readRegexEnd(chars: string[], start: number) {
  if (chars[start] !== '/') {
    return null;
  }

  let index = start + 1;
  while (index < chars.length) {
    const char = chars[index];
    if (char === '\\') {
      index += 2;
      continue;
    }
    if (char === '/' && (index + 1 === chars.length || isSpace(chars[index + 1]))) {
      return index > start + 1 ? index : null;
    }
    index += 1;
  }

  return null;
}

function readToken(chars: string[], start: number): [Token, number] {
  const colon = readKeyEnd(chars, start);
  const key = colon === null ? null : chars.slice(start, colon).join('');
  const valueStart = colon === null ? start : colon + 1;

  const regexEnd = readRegexEnd(chars, valueStart);
  if (regexEnd !== null) {
    const value = chars.slice(valueStart + 1, regexEnd).join('');
    return [{ key, value, isQuoted: false, isRegex: true }, regexEnd + 1];
  }

  let value = '';
  let isQuoted = false;
  let isInsideQuotes = false;
  let isInsideList = key !== null && chars[valueStart] === '[';
  let index = valueStart;
  while (index < chars.length) {
    const char = chars[index]!;
    if (isSpace(char) && !isInsideQuotes && !isInsideList) {
      break;
    }
    if (char === ']') {
      isInsideList = false;
    }

    const next = chars[index + 1];
    if (char === '"') {
      isQuoted = true;
      isInsideQuotes = !isInsideQuotes;
    } else if (char === '\\' && isInsideQuotes && (next === '"' || next === '\\')) {
      index += 1;
      value += next;
    } else {
      value += char;
    }
    index += 1;
  }

  return [{ key, value, isQuoted, isRegex: false }, index];
}

function readNumber(value: string, sign: '>' | '<') {
  if (!value.startsWith(sign)) {
    return null;
  }

  const rest = value.slice(1);
  return rest.trim() !== '' && Number.isFinite(Number(rest)) ? rest : null;
}

function readTerm(token: Token, negate: boolean): QueryTerm | null {
  const { key, value, isQuoted, isRegex } = token;

  if (!value) {
    return null;
  }
  if (isRegex) {
    return { key, kind: 'regex', values: [value], negate };
  }
  if (isQuoted || key === null) {
    return { key, kind: 'text', values: [value], negate };
  }

  const list = value.match(/^\[(.*)\]$/);
  if (list) {
    const values = list[1]!
      .split(',')
      .map((item) => item.trim())
      .filter(Boolean);
    return values.length ? { key, kind: 'list', values, negate } : null;
  }

  const above = readNumber(value, '>');
  if (above !== null) {
    return { key, kind: 'above', values: [above], negate };
  }
  const below = readNumber(value, '<');
  if (below !== null) {
    return { key, kind: 'below', values: [below], negate };
  }

  return { key, kind: 'text', values: [value], negate };
}

export function parseQuery(query: string): QueryTerm[] {
  const chars = [...query];
  const terms: QueryTerm[] = [];
  let index = 0;

  while (index < chars.length) {
    if (isSpace(chars[index])) {
      index += 1;
      continue;
    }

    const negate = chars[index] === '-';
    const [token, end] = readToken(chars, negate ? index + 1 : index);
    index = end;

    const term = readTerm(token, negate);
    if (term) {
      terms.push(term);
    }
  }

  return terms;
}

function needsQuotes(term: QueryTerm, value: string) {
  if (/[\s"]/.test(value)) {
    return true;
  }
  if (term.key !== null) {
    return /^[[<>/]/.test(value);
  }

  return /^[-/]/.test(value) || readKeyEnd([...value], 0) !== null;
}

function formatValue(term: QueryTerm) {
  const [value = ''] = term.values;

  if (term.kind === 'regex') {
    return `/${value}/`;
  }
  if (term.kind === 'list') {
    return `[${term.values.join(',')}]`;
  }
  if (term.kind === 'above') {
    return `>${value}`;
  }
  if (term.kind === 'below') {
    return `<${value}`;
  }
  if (needsQuotes(term, value)) {
    return `"${value.replace(/(["\\])/g, '\\$1')}"`;
  }

  return value;
}

export function formatTerm(term: QueryTerm) {
  const sign = term.negate ? '-' : '';
  const key = term.key === null ? '' : `${term.key}:`;

  return `${sign}${key}${formatValue(term)}`;
}

export function isPlainText(text: string, terms: QueryTerm[]) {
  return (
    !text.includes('"') &&
    terms.every((term) => term.key === null && term.kind === 'text' && !term.negate)
  );
}

export function isOpenRegex(term: QueryTerm) {
  const [value = ''] = term.values;

  return term.kind === 'text' && /^\/.*[()[\]{}|*+?^$\\]/.test(value);
}

export function getRegexError(term: QueryTerm) {
  if (term.kind !== 'regex') {
    return null;
  }

  try {
    new RegExp(term.values[0] ?? '');
    return null;
  } catch {
    return `/${term.values[0]}/ is not a valid regex`;
  }
}

function isWrapped(value: string) {
  return value.length > 2 && value.startsWith('*') && value.endsWith('*') && !value.slice(1, -1).includes('*');
}

export function getOperator(term: QueryTerm): SearchOperator {
  const [value = ''] = term.values;

  if (term.kind === 'regex') {
    return 'regex';
  }
  if (term.kind === 'above') {
    return 'above';
  }
  if (term.kind === 'below') {
    return 'below';
  }
  if (term.key === null || term.key === ANY_KEY) {
    return term.negate ? 'not_contains' : 'contains';
  }
  if (term.kind === 'text' && value === '*') {
    return term.negate ? 'not_exists' : 'exists';
  }
  if (term.kind === 'text' && isWrapped(value)) {
    return term.negate ? 'not_contains' : 'contains';
  }

  return term.negate ? 'is_not' : 'is';
}

export function getOperators(term: QueryTerm) {
  return term.key === null || term.key === ANY_KEY ? TEXT_OPERATORS : KEY_OPERATORS;
}

export function getTermValues(term: QueryTerm) {
  const [value = ''] = term.values;
  const operator = getOperator(term);

  if (operator === 'exists' || operator === 'not_exists') {
    return [];
  }
  if (term.key !== null && term.key !== ANY_KEY && (operator === 'contains' || operator === 'not_contains')) {
    return [value.slice(1, -1)];
  }

  return term.values;
}

export function canSetOperator(term: QueryTerm, operator: SearchOperator) {
  const values = getTermValues(term);

  if (operator === 'exists' || operator === 'not_exists') {
    return true;
  }
  if (operator === 'above' || operator === 'below') {
    return values.length === 1 && values[0]!.trim() !== '' && Number.isFinite(Number(values[0]));
  }

  return values.length > 0;
}

export function getEmptyText(term: QueryTerm, operator: SearchOperator) {
  const sign = operator === 'not_contains' || operator === 'is_not' ? '-' : '';
  const key = term.key === null ? '' : `${term.key}:`;

  if (operator === 'above') {
    return `${key}>`;
  }
  if (operator === 'below') {
    return `${key}<`;
  }

  return `${sign}${key}`;
}

export function setOperator(term: QueryTerm, operator: SearchOperator): QueryTerm {
  const { key } = term;
  const values = getTermValues(term);
  const negate = operator === 'not_contains' || operator === 'is_not' || operator === 'not_exists';
  const isKey = key !== null && key !== ANY_KEY;

  if (operator === 'exists' || operator === 'not_exists') {
    return { key, kind: 'text', values: ['*'], negate };
  }
  if (operator === 'regex') {
    return { key, kind: 'regex', values: [values.join('|')], negate: false };
  }
  if (operator === 'above' || operator === 'below') {
    return { key, kind: operator, values: values.slice(0, 1), negate: false };
  }
  if (isKey && (operator === 'contains' || operator === 'not_contains')) {
    return { key, kind: 'text', values: [`*${values[0] ?? ''}*`], negate };
  }

  return { key, kind: values.length > 1 ? 'list' : 'text', values, negate };
}

export function getKeyLabel(key: string | null) {
  if (key === null) {
    return 'Message';
  }
  if (key === ANY_KEY) {
    return 'Any field';
  }

  return KEY_LABELS[key] ?? key;
}

export function getTermCondition(term: QueryTerm) {
  return `${getKeyLabel(term.key)} ${OPERATOR_LABELS[getOperator(term)]}`;
}

export function getTermValue(term: QueryTerm) {
  return getTermValues(term).join(', ');
}
