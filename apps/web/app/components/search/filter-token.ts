import { Node } from '@tiptap/core';
import type { Node as ProseMirrorNode } from '@tiptap/pm/model';
import { TextSelection } from '@tiptap/pm/state';
import type { Transaction } from '@tiptap/pm/state';
import { ReactNodeViewRenderer } from '@tiptap/react';

import {
  FILTER_TOKEN_NODE,
  isFilterSearch,
  isFilterToken,
  isToken,
} from '~/utils/filter';
import type {
  FilterRange,
  FilterTokenAttrs,
  SearchFilterValue,
} from '~/utils/filter';

import { FilterTokenView } from './filter-token-view';

declare module '@tiptap/core' {
  interface Commands<ReturnType> {
    filterToken: {
      insertFilterToken: (
        attrs: FilterTokenAttrs,
        range?: FilterRange
      ) => ReturnType;
      updateFilterToken: (attrs: FilterTokenAttrs) => ReturnType;
      removeFilterToken: (key: string) => ReturnType;
      syncFilterTokens: (
        filters: Record<string, SearchFilterValue | undefined>
      ) => ReturnType;
      clearFilterSearch: () => ReturnType;
      placeFilterCaret: (key: string, side: 'before' | 'after') => ReturnType;
    };
  }
}

function getTokens(doc: ProseMirrorNode) {
  const tokens: { node: ProseMirrorNode; pos: number }[] = [];
  doc.forEach((node, pos) => {
    if (isFilterToken(node)) {
      tokens.push({ node, pos });
    }
  });
  return tokens;
}

function findToken(doc: ProseMirrorNode, key: string) {
  let token: { node: ProseMirrorNode; pos: number } | undefined;
  doc.forEach((node, pos) => {
    if (isToken(node) && node.attrs.key === key) {
      token = { node, pos };
    }
  });
  return token;
}

function deleteText(tr: Transaction, isSearch: boolean) {
  const ranges: FilterRange[] = [];
  tr.doc.forEach((node, pos) => {
    if (node.isText && isFilterSearch(node) === isSearch) {
      ranges.push({ from: pos, to: pos + node.nodeSize });
    }
  });

  for (const { from, to } of ranges.reverse()) {
    tr.delete(from, to);
  }
}

function moveCaretToEnd(tr: Transaction) {
  return tr.setSelection(TextSelection.create(tr.doc, tr.doc.content.size));
}

export function markAsSync(tr: Transaction) {
  return moveCaretToEnd(tr)
    .setMeta('preventUpdate', true)
    .setMeta('addToHistory', false);
}

export const FilterToken = Node.create({
  name: FILTER_TOKEN_NODE,
  group: 'inline',
  inline: true,
  atom: true,
  selectable: false,

  addAttributes() {
    return {
      key: { default: null },
      values: { default: [], rendered: false },
      negate: { default: false, rendered: false },
    };
  },

  parseHTML() {
    return [{ tag: 'span[data-filter-token]' }];
  },

  renderHTML({ HTMLAttributes }) {
    return ['span', { ...HTMLAttributes, 'data-filter-token': '' }];
  },

  addNodeView() {
    return ReactNodeViewRenderer(FilterTokenView, { as: 'span' });
  },

  addCommands() {
    return {
      insertFilterToken:
        (attrs, range) =>
        ({ tr }) => {
          const end = tr.doc.content.size;
          const { from, to } = range ?? { from: end, to: end };
          tr.delete(from, to);

          const previous = tr.doc.resolve(from).nodeBefore;
          const gap = previous?.isText && !/\s$/.test(previous.text ?? '');
          if (gap) {
            tr.insert(from, tr.doc.type.schema.text(' '));
          }

          const at = gap ? from + 1 : from;
          tr.insert(at, this.type.create(attrs));
          tr.setSelection(TextSelection.create(tr.doc, at + 1));
          return true;
        },

      updateFilterToken:
        (attrs) =>
        ({ tr }) => {
          const token = findToken(tr.doc, attrs.key);
          if (!token) {
            return false;
          }
          tr.setNodeMarkup(token.pos, undefined, attrs);
          return true;
        },

      removeFilterToken:
        (key) =>
        ({ tr }) => {
          const token = findToken(tr.doc, key);
          if (!token) {
            return false;
          }
          tr.delete(token.pos, token.pos + token.node.nodeSize);
          moveCaretToEnd(tr);
          return true;
        },

      syncFilterTokens:
        (filters) =>
        ({ tr }) => {
          const tokens = getTokens(tr.doc);
          for (const { node, pos } of [...tokens].reverse()) {
            const { key, ...current } = node.attrs as FilterTokenAttrs;
            const value = filters[key];
            if (!value?.values.length) {
              if (current.values.length) {
                tr.delete(pos, pos + node.nodeSize);
              }
            } else if (
              value.negate !== current.negate ||
              value.values.join(',') !== current.values.join(',')
            ) {
              tr.setNodeMarkup(pos, undefined, { key, ...value });
            }
          }

          const missing = Object.entries(filters).flatMap(([key, value]) =>
            value?.values.length &&
            !tokens.some(({ node }) => node.attrs.key === key)
              ? [this.type.create({ key, ...value })]
              : []
          );
          tr.insert(tr.doc.content.size, missing);
          if (!tr.docChanged) {
            return false;
          }

          markAsSync(tr);
          return true;
        },

      clearFilterSearch:
        () =>
        ({ tr }) => {
          deleteText(tr, true);
          return true;
        },

      placeFilterCaret:
        (key, side) =>
        ({ tr }) => {
          const token = findToken(tr.doc, key);
          if (!token) {
            return false;
          }

          const pos =
            side === 'before' ? token.pos : token.pos + token.node.nodeSize;
          tr.setSelection(TextSelection.create(tr.doc, pos));
          return true;
        },
    };
  },
});
