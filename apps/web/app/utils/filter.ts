import type { Node as ProseMirrorNode } from '@tiptap/pm/model';
import type { EditorState } from '@tiptap/pm/state';

import {
  ANY_KEY,
  formatTerm,
  getOperators,
  getRegexError,
  isOpenRegex,
  getTermCondition,
  getTermValue,
  isPlainText,
  OPERATOR_LABELS,
  parseQuery,
} from '~/utils/log-search';
import type { QueryTerm, SearchOperator } from '~/utils/log-search';

export const FILTER_TOKEN_NODE = 'filterToken';
export const QUERY_TOKEN_NODE = 'queryToken';
export const FILTER_SEARCH_MARK = 'filterSearch';

export type SearchFilterField = {
  key: string;
  label: string;
  verb?: string;
  multiple: boolean;
  negatable?: boolean;
  options: { value: string; label: string }[];
  plural?: string;
  onSearchChange?: (text: string) => void;
  isLoading?: boolean;
  isFailed?: boolean;
  onRetry?: () => void;
};

export type SearchFilterValue = {
  values: string[];
  negate: boolean;
};

export type SearchFilterChange = {
  key: string;
  value: SearchFilterValue | null;
};

export type EditPart = 'condition' | 'values';

export type EditOptions = {
  key: string;
  part: EditPart;
};

export type FilterEditing = EditOptions & {
  snapshot: SearchFilterValue | null;
};

export type FilterTokenAttrs = SearchFilterValue & {
  key: string;
};

export type QueryTokenAttrs = {
  key: string;
  term: QueryTerm;
};

export type FilterItem =
  | { kind: 'field'; key: string; field: SearchFilterField; negate: boolean }
  | { kind: 'value'; key: string; value: string; label: string }
  | { kind: 'operator'; key: string; negate: boolean; label: string }
  | { kind: 'terms'; key: string; terms: QueryTerm[]; label: string }
  | {
      kind: 'query-operator';
      key: string;
      operator: SearchOperator;
      label: string;
    };

type GetConditionLabelOptions = {
  field: SearchFilterField;
  negate: boolean;
};

export function getConditionLabel(options: GetConditionLabelOptions) {
  const { field, negate } = options;

  if (!field.verb) {
    return field.label;
  }
  return `${field.label} ${field.verb}${negate ? ' not' : ''}`;
}

export function getValueLabel(
  field: SearchFilterField,
  value: SearchFilterValue
) {
  return value.values
    .map(
      (item) =>
        field.options.find((option) => option.value === item)?.label ?? item
    )
    .join(', ');
}

function includesText(label: string, text: string) {
  return label.toLowerCase().includes(text.trim().toLowerCase());
}

function getTermLabel(term: QueryTerm) {
  return `${getTermCondition(term)} ${getTermValue(term)}`.trim();
}

function toTermsItem(term: QueryTerm, key: string): FilterItem {
  return { kind: 'terms', key, terms: [term], label: getTermLabel(term) };
}

export function getTextError(text: string) {
  return parseQuery(text).map(getRegexError).find(Boolean) ?? null;
}

export function hasOpenRegex(text: string) {
  return parseQuery(text).some(isOpenRegex);
}

export function getTextItems(text: string): FilterItem[] {
  const terms = parseQuery(text);
  if (!terms.length || getTextError(text)) {
    return [];
  }

  if (isPlainText(text, terms)) {
    const value = terms.map((term) => term.values[0]).join(' ');
    const term: QueryTerm = { key: null, kind: 'text', values: [value], negate: false };
    return [
      toTermsItem(term, 'terms:contains'),
      toTermsItem({ ...term, negate: true }, 'terms:not-contains'),
      toTermsItem({ ...term, key: ANY_KEY }, 'terms:any'),
    ];
  }

  const raw = text.replace(/\s+/g, ' ').trim();
  return [
    {
      kind: 'terms',
      key: 'terms:parsed',
      terms,
      label: terms.length === 1 ? getTermLabel(terms[0]!) : `Add ${terms.length} filters`,
    },
    toTermsItem({ key: null, kind: 'text', values: [raw], negate: false }, 'terms:text'),
  ];
}

export function getQueryOperatorItems(term: QueryTerm): FilterItem[] {
  return getOperators(term).map((operator) => ({
    kind: 'query-operator',
    key: `query-operator:${operator}`,
    operator,
    label: OPERATOR_LABELS[operator],
  }));
}

export function getItemLabel(item: FilterItem) {
  if (item.kind !== 'field') {
    return item.label;
  }
  if (item.negate) {
    return getConditionLabel({ field: item.field, negate: true });
  }

  return item.field.label;
}

export function getItemIndicator(item: FilterItem, field: SearchFilterField | null) {
  if (item.kind === 'field' || item.kind === 'terms') {
    return 'none';
  }
  if (item.kind === 'value' && field?.multiple) {
    return 'checkbox';
  }

  return 'check';
}

type IsItemCheckedOptions = {
  item: FilterItem;
  value: SearchFilterValue;
  operator: SearchOperator | null;
};

export function isItemChecked(options: IsItemCheckedOptions) {
  const { item, value, operator } = options;

  if (item.kind === 'value') {
    return value.values.includes(item.value);
  }
  if (item.kind === 'operator') {
    return value.negate === item.negate;
  }
  if (item.kind === 'query-operator') {
    return item.operator === operator;
  }

  return false;
}

export function hasSeparatorBefore(items: FilterItem[], index: number, isEmpty: boolean) {
  const item = items[index]!;
  const previous = items[index - 1];

  if (item.kind === 'operator') {
    return previous?.kind === 'value' || (index === 0 && isEmpty);
  }
  if (item.kind === 'field') {
    return previous?.kind === 'terms';
  }

  return false;
}

type GetFilterItemsOptions = {
  fields: SearchFilterField[];
  usedKeys: string[];
  field: SearchFilterField | null;
  part: EditPart;
  text: string;
};

export function getFilterItems(options: GetFilterItemsOptions): FilterItem[] {
  const { fields, usedKeys, field, part, text } = options;

  if (!field) {
    const fieldItems = fields
      .filter(
        (item) => !usedKeys.includes(item.key) && includesText(item.label, text)
      )
      .flatMap((item): FilterItem[] => [
        { kind: 'field', key: `field:${item.key}`, field: item, negate: false },
        ...(item.negatable && text.trim()
          ? [
              {
                kind: 'field' as const,
                key: `field:${item.key}:not`,
                field: item,
                negate: true,
              },
            ]
          : []),
      ]);
    return [...getTextItems(text), ...fieldItems];
  }
  const operators: FilterItem[] = field.negatable
    ? [
        { kind: 'operator', key: 'operator:is', negate: false, label: 'Is' },
        {
          kind: 'operator',
          key: 'operator:not',
          negate: true,
          label: 'Is not',
        },
      ]
    : [];
  if (part === 'condition') {
    return operators;
  }
  const values: FilterItem[] = field.options
    .filter((option) => includesText(option.label, text))
    .map((option) => ({
      kind: 'value',
      key: `value:${option.value}`,
      ...option,
    }));
  return [...values, ...operators];
}

export type FilterDoc = {
  tokens: FilterTokenAttrs[];
  queryTokens: QueryTokenAttrs[];
  query: string;
  search: string;
};

export type FilterRange = {
  from: number;
  to: number;
};

export type FilterCaret = FilterRange & {
  text: string;
};

export function isFilterToken(node: ProseMirrorNode | null | undefined) {
  return node?.type.name === FILTER_TOKEN_NODE;
}

export function isQueryToken(node: ProseMirrorNode | null | undefined) {
  return node?.type.name === QUERY_TOKEN_NODE;
}

export function isToken(node: ProseMirrorNode | null | undefined) {
  return isFilterToken(node) || isQueryToken(node);
}

export function isFilterSearch(node: ProseMirrorNode | null | undefined) {
  return (
    !!node?.isText &&
    node.marks.some((mark) => mark.type.name === FILTER_SEARCH_MARK)
  );
}

export function getQuery(segments: string[]) {
  return segments
    .map((segment) => segment.trim())
    .filter(Boolean)
    .join(' ');
}

export function readDoc(doc: ProseMirrorNode): FilterDoc {
  const tokens: FilterTokenAttrs[] = [];
  const queryTokens: QueryTokenAttrs[] = [];
  const segments = [''];
  let search = '';

  doc.forEach((node) => {
    if (isFilterToken(node)) {
      tokens.push(node.attrs as FilterTokenAttrs);
      segments.push('');
      return;
    }
    if (isQueryToken(node)) {
      const attrs = node.attrs as QueryTokenAttrs;
      queryTokens.push(attrs);
      segments.push(formatTerm(attrs.term), '');
      return;
    }
    if (isFilterSearch(node)) {
      search += node.text;
      return;
    }
    if (node.isText) {
      segments[segments.length - 1] += node.text;
    }
  });

  return { tokens, queryTokens, query: getQuery(segments), search };
}

export function readCaret(state: EditorState): FilterCaret {
  const caret = state.selection.from;
  let start = 0;
  let end = state.doc.content.size;

  state.doc.forEach((node, pos) => {
    if (!isToken(node)) {
      return;
    }
    if (pos + node.nodeSize <= caret) {
      start = pos + node.nodeSize;
    } else if (pos >= caret) {
      end = Math.min(end, pos);
    }
  });

  const raw = state.doc.textBetween(start, end);
  const leading = raw.length - raw.trimStart().length;
  const trailing = raw.length - raw.trimEnd().length;

  return {
    text: raw.trim(),
    from: start + leading,
    to: Math.max(start + leading, end - trailing),
  };
}

function toFilterValue(token: FilterTokenAttrs | undefined) {
  return token?.values.length
    ? { values: token.values, negate: token.negate }
    : null;
}

type GetFilterChangesOptions = {
  before: FilterTokenAttrs[];
  after: FilterTokenAttrs[];
};

export function getFilterChanges(options: GetFilterChangesOptions) {
  const { before, after } = options;

  const keys = new Set([...before, ...after].map((token) => token.key));
  return [...keys].flatMap((key): SearchFilterChange[] => {
    const previous = toFilterValue(before.find((token) => token.key === key));
    const value = toFilterValue(after.find((token) => token.key === key));
    return JSON.stringify(previous) === JSON.stringify(value)
      ? []
      : [{ key, value }];
  });
}

export function readListParam(value: string | null) {
  return value ? value.split(',').filter(Boolean) : [];
}
