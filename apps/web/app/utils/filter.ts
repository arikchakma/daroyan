import type { Node as ProseMirrorNode } from '@tiptap/pm/model';

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
  text: string;
};

export function readDoc(doc: ProseMirrorNode): FilterDoc {
  const tokens: FilterTokenAttrs[] = [];
  let text = '';
  doc.forEach((node) => {
    if (node.type.name === 'filterToken') {
      tokens.push(node.attrs as FilterTokenAttrs);
    } else if (node.isText) {
      text += node.text;
    }
  });
  return { tokens, text };
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
