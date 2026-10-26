import { NodeViewWrapper } from '@tiptap/react';
import type { ReactNodeViewProps } from '@tiptap/react';
import { useContext } from 'react';

import { Icon } from '~/components/icons/icon';
import { cn } from '~/utils/classname';
import { getConditionLabel, getValueLabel } from '~/utils/filter';
import type { FilterTokenAttrs } from '~/utils/filter';

import { TokenContext } from './filter-token-context';

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
  const hasValues = value.values.length > 0;
  const condition = getConditionLabel({ field, negate: value.negate });

  return (
    <NodeViewWrapper
      as="span"
      data-filter-token=""
      contentEditable={false}
      className="inline-flex h-5 gap-px pr-1 align-middle text-caption whitespace-nowrap select-none"
    >
      {hasValues ? (
        <>
          <button
            type="button"
            aria-label={`Change ${field.label} Condition`}
            onMouseDown={(e) => e.preventDefault()}
            onClick={() =>
              context.onEdit({
                key,
                part: field.negatable ? 'condition' : 'values',
              })
            }
            className={cn(
              'flex items-center rounded-l-sm rounded-r-[2px] bg-filter-label pr-1 pl-1.5 text-text-primary outline-none hover:bg-filter-label-active',
              editPart === 'condition' && 'bg-filter-label-active'
            )}
          >
            {condition}
          </button>
          <span
            className={cn(
              'group flex items-center rounded-l-[2px] rounded-r-sm bg-filter-value pr-1.5 pl-1 text-filter-value-fg hover:rounded-r-xs hover:bg-filter-value-hover hover:text-filter-value-fg-hover',
              editPart === 'values' &&
                'bg-filter-value-active hover:bg-filter-value-active'
            )}
          >
            <button
              type="button"
              aria-label={`${condition} ${getValueLabel(field, value)}`}
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => context.onEdit({ key, part: 'values' })}
              className="outline-none"
            >
              {getValueLabel(field, value)}
            </button>
            <button
              type="button"
              aria-label={`Remove ${field.label} Filter`}
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => context.onRemove(key)}
              className="-mr-0.5 ml-0.5 hidden size-4 items-center justify-center rounded-full outline-none group-hover:flex"
            >
              <Icon name="x" className="size-2.5" />
            </button>
          </span>
        </>
      ) : (
        <span className="flex items-center rounded-sm bg-filter-label-active px-1.5 text-text-primary">
          {condition}
        </span>
      )}
    </NodeViewWrapper>
  );
}
