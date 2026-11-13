import { NodeViewWrapper } from '@tiptap/react';
import type { ReactNodeViewProps } from '@tiptap/react';
import { useContext } from 'react';

import type { QueryTokenAttrs } from '~/utils/filter';
import { getKeyLabel, getTermCondition, getTermValue } from '~/utils/log-search';

import { TokenContext } from './filter-token-context';
import { TokenPill } from './token-pill';

export function QueryTokenView(props: ReactNodeViewProps) {
  const { node } = props;

  const context = useContext(TokenContext)!;
  const { key, term } = node.attrs as QueryTokenAttrs;
  const editPart = context.editing?.key === key ? context.editing.part : null;

  return (
    <NodeViewWrapper
      as="span"
      data-query-token=""
      data-token-key={key}
      contentEditable={false}
      className="inline-flex h-5 gap-px pr-1 align-middle text-caption whitespace-nowrap select-none"
    >
      <TokenPill
        name={getKeyLabel(term.key)}
        condition={getTermCondition(term)}
        value={getTermValue(term)}
        editPart={editPart}
        onEditCondition={() => context.onEdit({ key, part: 'condition' })}
        onEditValue={() => context.onEditText(key)}
        onRemove={() => context.onRemove(key)}
      />
    </NodeViewWrapper>
  );
}
