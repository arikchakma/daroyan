import { Node } from '@tiptap/core';
import { TextSelection } from '@tiptap/pm/state';
import type { Transaction } from '@tiptap/pm/state';
import { ReactNodeViewRenderer } from '@tiptap/react';

import {
  getQuery,
  isFilterSearch,
  isQueryToken,
  QUERY_TOKEN_NODE,
  readDoc,
} from '~/utils/filter';
import type { FilterRange, QueryTokenAttrs } from '~/utils/filter';
import { formatTerm, parseQuery } from '~/utils/log-search';
import type { QueryTerm } from '~/utils/log-search';

import { markAsSync } from './filter-token';
import { QueryTokenView } from './query-token-view';

declare module '@tiptap/core' {
  interface Commands<ReturnType> {
    queryToken: {
      insertQueryTokens: (terms: QueryTerm[], range: FilterRange) => ReturnType;
      updateQueryToken: (attrs: QueryTokenAttrs) => ReturnType;
      editQueryToken: (key: string, text?: string) => ReturnType;
      setFilterQuery: (query: string) => ReturnType;
    };
  }
}

let lastKey = 0;

function createKey() {
  lastKey += 1;
  return `query-${lastKey}`;
}

function findQueryToken(tr: Transaction, key: string) {
  let found: { pos: number; size: number; attrs: QueryTokenAttrs } | undefined;
  tr.doc.forEach((node, pos) => {
    if (isQueryToken(node) && node.attrs.key === key) {
      found = { pos, size: node.nodeSize, attrs: node.attrs as QueryTokenAttrs };
    }
  });
  return found;
}

function deleteQuery(tr: Transaction) {
  const ranges: FilterRange[] = [];
  tr.doc.forEach((node, pos) => {
    if ((node.isText && !isFilterSearch(node)) || isQueryToken(node)) {
      ranges.push({ from: pos, to: pos + node.nodeSize });
    }
  });

  for (const { from, to } of ranges.reverse()) {
    tr.delete(from, to);
  }
}

export const QueryToken = Node.create({
  name: QUERY_TOKEN_NODE,
  group: 'inline',
  inline: true,
  atom: true,
  selectable: false,

  addAttributes() {
    return {
      key: { default: null },
      term: { default: null, rendered: false },
    };
  },

  parseHTML() {
    return [{ tag: 'span[data-query-token]' }];
  },

  renderHTML({ HTMLAttributes }) {
    return ['span', { ...HTMLAttributes, 'data-query-token': '' }];
  },

  addNodeView() {
    return ReactNodeViewRenderer(QueryTokenView, { as: 'span' });
  },

  addCommands() {
    return {
      insertQueryTokens:
        (terms, range) =>
        ({ tr }) => {
          const nodes = terms.map((term) => this.type.create({ key: createKey(), term }));
          tr.replaceWith(range.from, range.to, nodes);

          tr.setSelection(TextSelection.create(tr.doc, range.from + nodes.length)).scrollIntoView();
          return true;
        },

      updateQueryToken:
        (attrs) =>
        ({ tr }) => {
          const token = findQueryToken(tr, attrs.key);
          if (!token) {
            return false;
          }

          tr.setNodeMarkup(token.pos, undefined, attrs);
          return true;
        },

      editQueryToken:
        (key, text) =>
        ({ tr }) => {
          const token = findQueryToken(tr, key);
          if (!token) {
            return false;
          }

          const value = text ?? formatTerm(token.attrs.term);
          tr.replaceWith(token.pos, token.pos + token.size, tr.doc.type.schema.text(value));
          tr.setSelection(TextSelection.create(tr.doc, token.pos + value.length)).scrollIntoView();
          return true;
        },

      setFilterQuery:
        (query) =>
        ({ tr }) => {
          if (readDoc(tr.doc).query === getQuery([query])) {
            return false;
          }

          deleteQuery(tr);
          const nodes = parseQuery(query).map((term) =>
            this.type.create({ key: createKey(), term })
          );
          tr.insert(tr.doc.content.size, nodes);

          markAsSync(tr).scrollIntoView();
          return true;
        },
    };
  },
});
