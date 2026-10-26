import type { EditorState } from '@tiptap/pm/state';
import { EditorContent } from '@tiptap/react';
import { useEffect, useId, useLayoutEffect, useRef, useState } from 'react';

import { Icon } from '~/components/icons/icon';
import { useSearchFilterEditor } from '~/hooks/use-search-filter-editor';
import { getFilterChanges, getFilterItems, readDoc } from '~/utils/filter';
import type {
  EditOptions,
  FilterDoc,
  FilterEditing,
  FilterItem,
  SearchFilterChange,
  SearchFilterField,
  SearchFilterValue,
} from '~/utils/filter';

import { FilterList } from './filter-list';
import { TokenContext } from './filter-token-context';

type SearchFilterProps = {
  label: string;
  fields: SearchFilterField[];
  filters: Record<string, SearchFilterValue | undefined>;
  onFilterChange: (options: SearchFilterChange) => void;
  query: string;
  onQueryChange: (query: string) => void;
};

export function SearchFilter(props: SearchFilterProps) {
  const { label, fields, filters, onFilterChange, query, onQueryChange } =
    props;

  const rootRef = useRef<HTMLDivElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const listId = useId();
  const [isOpen, setIsOpen] = useState(false);
  const [editing, setEditing] = useState<FilterEditing | null>(null);
  const [activeKey, setActiveKey] = useState<string | null>(null);
  const [{ tokens, text }, setDoc] = useState<FilterDoc>({
    tokens: [],
    text: '',
  });

  const field = fields.find((item) => item.key === editing?.key) ?? null;
  const part = editing?.part ?? 'values';
  const value: SearchFilterValue = tokens.find(
    (token) => token.key === editing?.key
  ) ?? { values: [], negate: false };
  const items = getFilterItems({
    fields,
    usedKeys: tokens.map((token) => token.key),
    field,
    part,
    text,
  }).filter(
    (item) => !((field?.isLoading || field?.isFailed) && item.kind === 'value')
  );
  const isEmpty = !!field && part === 'values' && items[0]?.kind !== 'value';
  const isListVisible = isOpen && (items.length > 0 || isEmpty);

  // Typing in the field list clears the highlight. Elsewhere it moves to the first item, but a value that was just picked keeps it.
  const mode = editing ? `${editing.key}:${part}` : 'fields';
  const [previous, setPrevious] = useState({ mode, text });
  if (previous.mode !== mode || previous.text !== text) {
    setPrevious({ mode, text });
    const keep =
      previous.mode === mode &&
      !text &&
      items.some((item) => item.key === activeKey);
    setActiveKey(!field ? null : keep ? activeKey : (items[0]?.key ?? null));
  }
  const activeIndex = items.findIndex((item) => item.key === activeKey);
  const isAddVisible =
    tokens.some((token) => token.values.length) &&
    fields.some((item) => !tokens.some((token) => token.key === item.key)) &&
    !text &&
    !(isListVisible && !editing);

  const editor = useSearchFilterEditor({
    editorProps: {
      attributes: {
        class:
          'min-w-0 flex-1 overflow-hidden py-1 text-body leading-5 whitespace-pre text-text-primary outline-none',
        role: 'combobox',
        'aria-label': label,
        'aria-autocomplete': 'list',
        'aria-expanded': String(isListVisible),
        'aria-controls': listId,
        ...(isListVisible && activeIndex >= 0
          ? { 'aria-activedescendant': `${listId}-${activeIndex}` }
          : {}),
      },
      handleKeyDown: (view, event) => handleKeyDown(event, view.state),
      handleClick: () => {
        setIsOpen(true);
        return false;
      },
    },
    onTransaction: ({ editor: current }) => setDoc(readDoc(current.state.doc)),
    onFocus: () => setIsOpen(true),
    onBlur: ({ event }) => {
      if (listRef.current?.contains(event.relatedTarget as Node | null)) {
        return;
      }
      stopEditing();
      setIsOpen(false);
    },
    onUpdate: ({ editor: current, transaction }) => {
      const before = readDoc(transaction.before);
      const after = readDoc(current.state.doc);
      for (const change of getFilterChanges({
        before: before.tokens,
        after: after.tokens,
      })) {
        onFilterChange(change);
      }
      if (after.text !== before.text) {
        setIsOpen(true);
      }
      if (!editing) {
        onQueryChange(after.text);
        return;
      }
      if (after.text !== before.text) {
        fields
          .find((item) => item.key === editing.key)
          ?.onSearchChange?.(after.text);
      }
      if (!after.tokens.some((token) => token.key === editing.key)) {
        setEditing(null);
      }
    },
  });

  // The doc changes first and the props follow later. Sync only when the props change, so a late render does not undo an edit.
  const filtersKey = JSON.stringify(filters);
  useEffect(() => {
    editor?.commands.syncFilterTokens(JSON.parse(filtersKey));
  }, [editor, filtersKey]);

  const isEditing = editing !== null;
  useEffect(() => {
    if (!isEditing) {
      editor?.commands.setFilterText(query);
    }
  }, [editor, isEditing, query]);

  useEffect(() => {
    if (!editor) {
      return;
    }
    editor.commands.setFilterAddButton(
      isAddVisible
        ? () => {
            setEditing(null);
            setIsOpen(true);
            editor.commands.focus('end');
          }
        : null
    );
  }, [editor, isAddVisible]);

  useLayoutEffect(() => {
    const root = rootRef.current;
    const list = listRef.current;
    if (!editor || !root || !list) {
      return;
    }
    const index = tokens.findIndex((token) => token.key === editing?.key);
    const token = root.querySelectorAll('[data-filter-token]')[index];
    const left = token
      ? token.getBoundingClientRect().left
      : editor.view.coordsAtPos(editor.state.doc.content.size).left;
    list.style.left = `${left - root.getBoundingClientRect().left - 4}px`;
  }, [editor, isListVisible, editing?.key, tokens, text]);

  function startEditing(options: EditOptions) {
    if (editing && editing.key !== options.key && !value.values.length) {
      editor?.commands.removeFilterToken(editing.key);
    }
    const token = tokens.find((item) => item.key === options.key);
    fields.find((item) => item.key === options.key)?.onSearchChange?.('');
    setEditing({ ...options, snapshot: token?.values.length ? token : null });
    setIsOpen(true);
    editor?.chain().setFilterText('').focus('end').run();
  }

  function stopEditing() {
    if (editing && !value.values.length) {
      editor?.commands.removeFilterToken(editing.key);
    }
    setEditing(null);
  }

  function commitToken() {
    setEditing(null);
    setIsOpen(true);
    editor?.commands.focus('end');
  }

  function choose(item: FilterItem) {
    if (item.kind === 'field') {
      editor?.commands.setFilterText('');
      editor?.commands.insertFilterToken({
        key: item.field.key,
        values: [],
        negate: item.negate,
      });
      startEditing({ key: item.field.key, part: 'values' });
      return;
    }
    if (!field) {
      return;
    }
    if (item.kind === 'operator') {
      editor?.commands.updateFilterToken({
        key: field.key,
        values: value.values,
        negate: item.negate,
      });
      if (part === 'condition') {
        setEditing(null);
      }
      return;
    }
    editor?.commands.setFilterText('');
    setActiveKey(item.key);
    if (!field.multiple) {
      editor?.commands.updateFilterToken({
        key: field.key,
        values: [item.value],
        negate: false,
      });
      setEditing(null);
      setIsOpen(false);
      return;
    }
    editor?.commands.updateFilterToken({
      key: field.key,
      negate: value.negate,
      values: value.values.includes(item.value)
        ? value.values.filter((entry) => entry !== item.value)
        : [...value.values, item.value],
    });
  }

  // Text input reaches React after the next key can arrive, so keys read the doc from the editor.
  function handleKeyDown(event: KeyboardEvent, state: EditorState) {
    const doc = readDoc(state.doc);
    const editedToken = doc.tokens.find((token) => token.key === editing?.key);
    switch (event.key) {
      case 'ArrowDown':
      case 'ArrowUp': {
        const count = items.length;
        if (!isListVisible || count === 0) {
          setIsOpen(true);
          return true;
        }
        const step = event.key === 'ArrowDown' ? 1 : -1;
        const next =
          activeIndex < 0
            ? step > 0
              ? 0
              : count - 1
            : (activeIndex + step + count) % count;
        setActiveKey(items[next]!.key);
        return true;
      }
      case 'ArrowLeft':
      case 'ArrowRight': {
        const { from, empty } = state.selection;
        if (!editor || !empty || event.altKey || event.metaKey) {
          return false;
        }
        const end = state.doc.content.size;
        const step = event.key === 'ArrowLeft' ? -1 : 1;
        // The editor moves the caret itself, so the next key sees where it is.
        if (step < 0 ? from > end - doc.text.length : from < end) {
          editor.commands.setTextSelection(from + step);
          return true;
        }
        const index = editing
          ? doc.tokens.findIndex((token) => token.key === editing.key)
          : doc.tokens.length;
        const next = doc.tokens[index + step];
        if (next) {
          startEditing({ key: next.key, part: 'values' });
        } else if (editing && step > 0) {
          stopEditing();
          setIsOpen(true);
        }
        return true;
      }
      case 'Enter': {
        const item = items[activeIndex];
        if (isListVisible && item) {
          choose(item);
        }
        return true;
      }
      case 'Escape':
        if (!editing) {
          setIsOpen(false);
        } else if (editing.snapshot) {
          editor?.commands.updateFilterToken({
            key: editing.key,
            ...editing.snapshot,
          });
          setEditing(null);
        } else {
          editor?.commands.removeFilterToken(editing.key);
          setEditing(null);
        }
        return true;
      case ' ':
        if (!editing || doc.text || !editedToken?.values.length) {
          return false;
        }
        commitToken();
        return true;
      case 'Backspace': {
        if (doc.text) {
          return false;
        }
        const last = doc.tokens.at(-1);
        if (editing) {
          editor?.commands.removeFilterToken(editing.key);
          setEditing(null);
          setIsOpen(true);
        } else if (last) {
          startEditing({ key: last.key, part: 'values' });
        }
        return true;
      }
      default:
        return false;
    }
  }

  return (
    <TokenContext.Provider
      value={{
        fields,
        editing,
        onEdit: startEditing,
        onRemove: (key) => {
          editor?.commands.removeFilterToken(key);
          if (key === editing?.key) {
            setEditing(null);
          }
        },
      }}
    >
      <div
        ref={rootRef}
        className="relative flex h-7 min-w-0 flex-1 items-center gap-1.5 rounded-sm pr-2.5 pl-2"
        onMouseDown={(e) => {
          if (e.target === e.currentTarget) {
            e.preventDefault();
            setIsOpen(true);
            editor?.commands.focus('end');
          }
        }}
      >
        <Icon name="search" className="shrink-0 text-text-tertiary" />

        <div className="relative flex min-w-0 flex-1 items-center">
          <EditorContent editor={editor} className="flex min-w-0 flex-1" />
          {tokens.length === 0 && !text && (
            <span
              aria-hidden="true"
              className="pointer-events-none absolute inset-y-0 left-0 flex items-center text-body text-text-tertiary"
            >
              {label}
            </span>
          )}
        </div>

        {isListVisible && (
          <FilterList
            ref={listRef}
            id={listId}
            items={items}
            activeKey={activeKey}
            field={field}
            part={part}
            value={value}
            text={text}
            isEmpty={isEmpty}
            onHover={setActiveKey}
            onSelect={choose}
            onRetry={() => {
              field?.onRetry?.();
              editor?.commands.focus('end');
            }}
            onLeave={(target) => {
              if (rootRef.current?.contains(target)) {
                return;
              }
              stopEditing();
              setIsOpen(false);
            }}
          />
        )}
      </div>
    </TokenContext.Provider>
  );
}
