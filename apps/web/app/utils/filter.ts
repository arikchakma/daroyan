import type { Node as ProseMirrorNode } from '@tiptap/pm/model';
import type { EditorState } from '@tiptap/pm/state';

export const FILTER_TOKEN_NODE = 'filterToken';
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

export type FilterItem =
  | { kind: 'field'; key: string; field: SearchFilterField; negate: boolean }
  | { kind: 'value'; key: string; value: string; label: string }
  | { kind: 'operator'; key: string; negate: boolean; label: string };

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
    return fields
      .filter(
        (item) => !usedKeys.includes(item.key) && includesText(item.label, text)
      )
      .flatMap((item) => [
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
  const segments = [''];
  let search = '';

  doc.forEach((node) => {
    if (isFilterToken(node)) {
      tokens.push(node.attrs as FilterTokenAttrs);
      segments.push('');
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

  return { tokens, query: getQuery(segments), search };
}

export function readCaret(state: EditorState): FilterCaret {
  const caret = state.selection.from;
  let start = 0;
  let end = state.doc.content.size;

  state.doc.forEach((node, pos) => {
    if (!isFilterToken(node)) {
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
