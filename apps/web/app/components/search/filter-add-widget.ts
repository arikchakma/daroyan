import { Extension } from '@tiptap/core';
import { Plugin, PluginKey } from '@tiptap/pm/state';
import { Decoration, DecorationSet } from '@tiptap/pm/view';
import { ReactRenderer } from '@tiptap/react';

import { FilterAddButton } from './filter-add-button';

declare module '@tiptap/core' {
  interface Commands<ReturnType> {
    filterAddWidget: {
      setFilterAddButton: (onAdd: (() => void) | null) => ReturnType;
    };
  }
}

const filterAddKey = new PluginKey<(() => void) | null>('filterAddWidget');

export const FilterAddWidget = Extension.create({
  name: 'filterAddWidget',

  addCommands() {
    return {
      setFilterAddButton:
        (onAdd) =>
        ({ tr }) => {
          tr.setMeta(filterAddKey, onAdd);
          return true;
        },
    };
  },

  addProseMirrorPlugins() {
    const { editor } = this;
    const renderers = new WeakMap<Node, ReactRenderer>();

    return [
      new Plugin({
        key: filterAddKey,
        state: {
          init: () => null,
          apply: (tr, onAdd) => {
            const next = tr.getMeta(filterAddKey);
            return next === undefined ? onAdd : next;
          },
        },
        props: {
          decorations(state) {
            const onAdd = filterAddKey.getState(state);
            if (!onAdd) {
              return null;
            }
            // A positive side draws the widget after the caret at the same position.
            const widget = Decoration.widget(
              state.doc.content.size,
              () => {
                const renderer = new ReactRenderer(FilterAddButton, {
                  editor,
                  props: { onClick: onAdd },
                  as: 'span',
                });
                renderer.element.contentEditable = 'false';
                renderers.set(renderer.element, renderer);
                return renderer.element;
              },
              {
                side: 1,
                key: 'filter-add',
                ignoreSelection: true,
                destroy: (node) => renderers.get(node)?.destroy(),
              }
            );
            return DecorationSet.create(state.doc, [widget]);
          },
        },
      }),
    ];
  },
});
