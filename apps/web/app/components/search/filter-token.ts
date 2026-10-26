import { Node } from '@tiptap/core';
import type { Node as ProseMirrorNode } from '@tiptap/pm/model';
import { Plugin, TextSelection } from '@tiptap/pm/state';
import type { Transaction } from '@tiptap/pm/state';
import { ReactNodeViewRenderer } from '@tiptap/react';

import type { FilterTokenAttrs, SearchFilterValue } from '~/utils/filter';

import { FilterTokenView } from './filter-token-view';

declare module '@tiptap/core' {
  interface Commands<ReturnType> {
    filterToken: {
      insertFilterToken: (attrs: FilterTokenAttrs) => ReturnType;
      updateFilterToken: (attrs: FilterTokenAttrs) => ReturnType;
      removeFilterToken: (key: string) => ReturnType;
      syncFilterTokens: (
        filters: Record<string, SearchFilterValue | undefined>
      ) => ReturnType;
      setFilterText: (text: string) => ReturnType;
    };
  }
}

function getTokens(doc: ProseMirrorNode) {
  const tokens: { node: ProseMirrorNode; pos: number }[] = [];
  doc.forEach((node, pos) => {
    if (node.type.name === 'filterToken') {
      tokens.push({ node, pos });
    }
  });
  return tokens;
}

function getTokensEnd(doc: ProseMirrorNode) {
  const last = getTokens(doc).at(-1);
  return last ? last.pos + last.node.nodeSize : 0;
}

function moveCaretToEnd(tr: Transaction) {
  return tr.setSelection(TextSelection.create(tr.doc, tr.doc.content.size));
}

function markAsSync(tr: Transaction) {
  return moveCaretToEnd(tr)
    .setMeta('preventUpdate', true)
    .setMeta('addToHistory', false);
}

export const FilterToken = Node.create({
  name: 'filterToken',
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
        (attrs) =>
        ({ tr }) => {
          tr.insert(getTokensEnd(tr.doc), this.type.create(attrs));
          moveCaretToEnd(tr);
          return true;
        },

      updateFilterToken:
        (attrs) =>
        ({ tr }) => {
          const token = getTokens(tr.doc).find(
            ({ node }) => node.attrs.key === attrs.key
          );
          if (!token) {
            return false;
          }
          tr.setNodeMarkup(token.pos, undefined, attrs);
          return true;
        },

      removeFilterToken:
        (key) =>
        ({ tr }) => {
          const token = getTokens(tr.doc).find(
            ({ node }) => node.attrs.key === key
          );
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
          tr.insert(getTokensEnd(tr.doc), missing);
          if (!tr.docChanged) {
            return false;
          }
          markAsSync(tr);
          return true;
        },

      setFilterText:
        (text) =>
        ({ tr }) => {
          const from = getTokensEnd(tr.doc);
          if (tr.doc.textBetween(from, tr.doc.content.size) === text) {
            return false;
          }
          if (text) {
            tr.insertText(text, from, tr.doc.content.size);
          } else {
            tr.delete(from, tr.doc.content.size);
          }
          markAsSync(tr);
          return true;
        },
    };
  },

  addProseMirrorPlugins() {
    return [
      new Plugin({
        // The caret can move in front of a token. Text typed there moves to the end.
        appendTransaction(transactions, _oldState, state) {
          if (!transactions.some((tr) => tr.docChanged)) {
            return null;
          }
          const end = getTokensEnd(state.doc);
          const stray: { from: number; to: number; text: string }[] = [];
          state.doc.forEach((node, pos) => {
            if (node.isText && pos < end) {
              stray.push({
                from: pos,
                to: pos + node.nodeSize,
                text: node.text!,
              });
            }
          });
          if (stray.length === 0) {
            return null;
          }
          const tr = state.tr;
          for (const { from, to } of [...stray].reverse()) {
            tr.delete(from, to);
          }
          tr.insertText(
            stray.map((item) => item.text).join(''),
            tr.doc.content.size
          );
          return moveCaretToEnd(tr);
        },
      }),
    ];
  },
});
