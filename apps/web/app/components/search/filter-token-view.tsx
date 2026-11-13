import { NodeViewWrapper } from '@tiptap/react';
import type { ReactNodeViewProps } from '@tiptap/react';
import { useContext } from 'react';

import { getConditionLabel, getValueLabel } from '~/utils/filter';
import type { FilterTokenAttrs } from '~/utils/filter';

import { TokenContext } from './filter-token-context';
import { TokenPill } from './token-pill';

export function FilterTokenView(props: ReactNodeViewProps) {
  const { node } = props;

  const context = useContext(TokenContext)!;
  const value = node.attrs as FilterTokenAttrs;
  const { key } = value;
  const field = context.fields.find((item) => item.key === key);
  if (!field) {
    return null;
  }
  const editPart = context.editing?.key === key ? context.editing.part : null;
  const condition = getConditionLabel({ field, negate: value.negate });

  return (
    <NodeViewWrapper
      as="span"
      data-filter-token=""
      data-token-key={key}
      contentEditable={false}
      className="inline-flex h-5 gap-px pr-1 align-middle text-caption whitespace-nowrap select-none"
    >
      {value.values.length > 0 ? (
        <TokenPill
          name={field.label}
          condition={condition}
          value={getValueLabel(field, value)}
          editPart={editPart}
          onEditCondition={() =>
            context.onEdit({
              key,
              part: field.negatable ? 'condition' : 'values',
            })
          }
          onEditValue={() => context.onEdit({ key, part: 'values' })}
          onRemove={() => context.onRemove(key)}
        />
      ) : (
        <span className="flex items-center rounded-sm bg-filter-label-active px-1.5 text-text-primary">
          {condition}
        </span>
      )}
    </NodeViewWrapper>
  );
}
