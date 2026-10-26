import { Node } from '@tiptap/core';
import { useEditor } from '@tiptap/react';
import type { UseEditorOptions } from '@tiptap/react';

import { FilterAddWidget } from '~/components/search/filter-add-widget';
import { FilterToken } from '~/components/search/filter-token';

const FilterDocument = Node.create({
  name: 'doc',
  topNode: true,
  content: 'inline*',
});

const FilterText = Node.create({
  name: 'text',
  group: 'inline',
});

const EXTENSIONS = [FilterDocument, FilterText, FilterToken, FilterAddWidget];

export function useSearchFilterEditor(options: UseEditorOptions) {
  const { editorProps, ...rest } = options;

  return useEditor({
    extensions: EXTENSIONS,
    content: '',
    editorProps: {
      ...editorProps,
      handlePaste: (view, event) => {
        const pasted = event.clipboardData?.getData('text/plain') ?? '';
        view.dispatch(view.state.tr.insertText(pasted.replace(/\s+/g, ' ')));
        return true;
      },
    },
    ...rest,
  });
}
