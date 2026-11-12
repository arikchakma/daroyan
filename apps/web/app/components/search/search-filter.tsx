import type { EditorState } from '@tiptap/pm/state';
import { EditorContent } from '@tiptap/react';
import { useEffect, useId, useLayoutEffect, useRef, useState } from 'react';

import { Icon } from '~/components/icons/icon';
import { useSearchFilterEditor } from '~/hooks/use-search-filter-editor';
import { cn } from '~/utils/classname';
import {
  FILTER_SEARCH_MARK,
  getFilterChanges,
  getFilterItems,
  getQueryOperatorItems,
  getTextError,
  hasOpenRegex,
  isFilterSearch,
  isFilterToken,
  readCaret,
  readDoc,
} from '~/utils/filter';
import type {
  EditOptions,
  FilterCaret,
  FilterDoc,
  FilterEditing,
  FilterItem,
  FilterTokenAttrs,
  SearchFilterChange,
  SearchFilterField,
  SearchFilterValue,
} from '~/utils/filter';
import {
  canSetOperator,
  getEmptyText,
  getOperator,
  setOperator,
} from '~/utils/log-search';

import { FilterList } from './filter-list';
import { TokenContext } from './filter-token-context';

const TYPING_PAUSE_MS = 600;

type SearchFilterProps = {
  label: string;
  fields: SearchFilterField[];
  filters: Record<string, SearchFilterValue | undefined>;
  onFilterChange: (changes: SearchFilterChange[]) => void;
  query: string;
  onQueryChange: (query: string) => void;
  isSearching?: boolean;
};

export function SearchFilter(props: SearchFilterProps) {
  const {
    label,
    fields,
    filters,
    onFilterChange,
    query,
    onQueryChange,
    isSearching = false,
  } = props;

  const rootRef = useRef<HTMLDivElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const typingTimerRef = useRef<number | undefined>(undefined);
  const sentQueryRef = useRef<string | null>(null);
  const listId = useId();
  const [isOpen, setIsOpen] = useState(false);
  const [editing, setEditing] = useState<FilterEditing | null>(null);
  const [activeKey, setActiveKey] = useState<string | null>(null);
  const [isPicking, setIsPicking] = useState(false);
  const [isTyping, setIsTyping] = useState(false);
  const [{ tokens, queryTokens, query: docQuery, search }, setDoc] =
    useState<FilterDoc>({
      tokens: [],
      queryTokens: [],
      query: '',
    search: '',
  });
  const [caret, setCaret] = useState<FilterCaret>({ text: '', from: 0, to: 0 });

  const field = fields.find((item) => item.key === editing?.key) ?? null;
  const part = editing?.part ?? 'values';
  const value: SearchFilterValue = tokens.find(
    (token) => token.key === editing?.key
  ) ?? { values: [], negate: false };
  const queryToken =
    queryTokens.find((token) => token.key === editing?.key) ?? null;
  const operator = queryToken ? getOperator(queryToken.term) : null;
  const text = getListText();
  const isTextList = !editing && !isPicking;
  const error = isTextList ? getTextError(text) : null;
  const hint =
    isTextList && hasOpenRegex(text)
      ? 'Searching waits until the regex ends with /'
      : null;
  const items = queryToken
    ? getQueryOperatorItems(queryToken.term)
    : getFilterItems({
        fields,
        usedKeys: tokens.map((token) => token.key),
        field,
        part,
        text,
      }).filter(
        (item) =>
          !((field?.isLoading || field?.isFailed) && item.kind === 'value')
      );
  const isEmpty = !!field && part === 'values' && items[0]?.kind !== 'value';
  const isListVisible =
    isOpen && (items.length > 0 || isEmpty || !!error || !!hint);

  // Typing in the field list highlights a search option when there is one, or nothing. Elsewhere the highlight moves to the first item, but a value that was just picked keeps it.
  const mode = editing ? `${editing.key}:${part}` : 'fields';
  const [previous, setPrevious] = useState({ mode, text });
  if (previous.mode !== mode || previous.text !== text) {
    setPrevious({ mode, text });
    const keep =
      previous.mode === mode &&
      !text &&
      items.some((item) => item.key === activeKey);
    const firstTerms = items[0]?.kind === 'terms' ? items[0].key : null;
    const checkedOperator = operator ? `query-operator:${operator}` : null;
    const isPicker = !field && !queryToken;
    setActiveKey(
      isPicker
        ? firstTerms
        : (checkedOperator ?? (keep ? activeKey : (items[0]?.key ?? null)))
    );
  }
  const activeIndex = items.findIndex((item) => item.key === activeKey);
  const isAddVisible =
    (!!docQuery || tokens.some((token) => token.values.length)) &&
    fields.some((item) => !tokens.some((token) => token.key === item.key)) &&
    !editing &&
    !isTyping &&
    !isListVisible;

  const editor = useSearchFilterEditor({
    editorProps: {
      attributes: {
        class:
          'min-w-0 flex-1 scrollbar-none overflow-x-auto py-1 text-body leading-5 whitespace-pre! text-text-primary outline-none [&_.react-renderer]:whitespace-nowrap!',
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
      handleTextInput: (view, from, to, input) => {
        const mark = view.state.schema.marks[FILTER_SEARCH_MARK]!;
        const tr = view.state.tr.insertText(input, from, to);
        const end = from + input.length;

        if (editing) {
          tr.addMark(from, end, mark.create());
        } else {
          tr.removeMark(from, end, mark);
        }

        view.dispatch(tr);
        return true;
      },
      handleClick: (_view, _pos, event) => {
        const target = event.target as Element | null;
        if (!target?.closest('[data-token-key]')) {
          stopEditing();
        }
        setIsOpen(true);
        return false;
      },
    },
    onTransaction: ({ editor: current }) => {
      setDoc(readDoc(current.state.doc));
      setCaret(readCaret(current.state));
    },
    onFocus: () => setIsOpen(true),
    onBlur: ({ event }) => {
      if (listRef.current?.contains(event.relatedTarget as Node | null)) {
        return;
      }
      stopEditing();
      setIsOpen(false);
      setIsPicking(false);
    },
    onUpdate: ({ editor: current, transaction }) => {
      const before = readDoc(transaction.before);
      const after = readDoc(current.state.doc);
      const changes = getFilterChanges({
        before: before.tokens,
        after: after.tokens,
      });
      if (changes.length) {
        onFilterChange(changes);
      }

      const isQueryReady =
        !getTextError(after.query) && !hasOpenRegex(after.query);
      if (after.query !== before.query && isQueryReady) {
        sentQueryRef.current = after.query;
        onQueryChange(after.query);
      }
      if (transaction.before.textContent !== current.state.doc.textContent) {
        setIsOpen(true);
        setIsPicking(false);
      }

      if (!editing) {
        return;
      }
      if (after.search !== before.search) {
        fields
          .find((item) => item.key === editing.key)
          ?.onSearchChange?.(after.search);
      }
      const isEditedTokenGone = ![...after.tokens, ...after.queryTokens].some(
        (token) => token.key === editing.key
      );
      if (isEditedTokenGone) {
        setEditing(null);
      }
    },
  });

  // The doc changes first and the props follow later. Sync only when the props change, so a late render does not undo an edit.
  const filtersKey = JSON.stringify(filters);
  useEffect(() => {
    editor?.commands.syncFilterTokens(JSON.parse(filtersKey));
  }, [editor, filtersKey]);

  // The bar sends its query up and gets it back a render later, while typing may have moved on. Only a query from outside the bar rebuilds the pills.
  useEffect(() => {
    if (!editor || query === sentQueryRef.current) {
      return;
    }

    sentQueryRef.current = query;
    editor.commands.setFilterQuery(query);
  }, [editor, query]);

  useEffect(() => {
    if (!editor) {
      return;
    }
    editor.commands.setFilterAddButton(
      isAddVisible
        ? () => {
            setIsPicking(true);
            setIsOpen(true);
            editor.commands.focus('end');
          }
        : null
    );
  }, [editor, isAddVisible]);

  useEffect(() => () => window.clearTimeout(typingTimerRef.current), []);

  useLayoutEffect(() => {
    const root = rootRef.current;
    const list = listRef.current;
    if (!editor || !root || !list) {
      return;
    }

    const token = editing
      ? root.querySelector(`[data-token-key="${CSS.escape(editing.key)}"]`)
      : null;
    const pos = isPicking ? editor.state.doc.content.size : caret.from;
    const left = token
      ? token.getBoundingClientRect().left
      : editor.view.coordsAtPos(pos).left;
    list.style.left = `${left - root.getBoundingClientRect().left - 4}px`;
  }, [editor, isListVisible, editing, tokens, queryTokens, isPicking, caret.from]);

  function getListText() {
    if (editing) {
      return search;
    }
    if (isPicking) {
      return '';
    }
    return caret.text;
  }

  function markTyping() {
    setIsTyping(true);
    window.clearTimeout(typingTimerRef.current);
    typingTimerRef.current = window.setTimeout(
      () => setIsTyping(false),
      TYPING_PAUSE_MS
    );
  }

  function startEditing(options: EditOptions) {
    if (editing && editing.key !== options.key && field && !value.values.length) {
      editor?.commands.removeFilterToken(editing.key);
    }

    const token = tokens.find((item) => item.key === options.key);
    fields.find((item) => item.key === options.key)?.onSearchChange?.('');
    setEditing({ ...options, snapshot: token?.values.length ? token : null });
    setIsPicking(false);
    setIsOpen(true);
    editor
      ?.chain()
      .clearFilterSearch()
      .placeFilterCaret(options.key, 'after')
      .scrollIntoView()
      .focus()
      .run();
  }

  function stopEditing() {
    if (!editing) {
      return;
    }

    const chain = editor?.chain().clearFilterSearch();
    if (field && !value.values.length) {
      chain?.removeFilterToken(editing.key);
    }
    chain?.run();
    setEditing(null);
  }

  function commitToken(options: { isOpen: boolean }) {
    editor?.chain().clearFilterSearch().focus('end').run();
    setEditing(null);
    setIsOpen(options.isOpen);
  }

  function editAsText(key: string, text?: string) {
    setEditing(null);
    setIsOpen(true);
    editor?.chain().clearFilterSearch().editQueryToken(key, text).focus().run();
  }

  function choose(item: FilterItem) {
    if (item.kind === 'terms') {
      editor?.chain().insertQueryTokens(item.terms, caret).focus().run();
      requestAnimationFrame(() => editor?.commands.scrollIntoView());
      setIsOpen(false);
      return;
    }
    if (item.kind === 'query-operator') {
      if (!queryToken) {
        return;
      }

      const { key, term } = queryToken;
      if (!canSetOperator(term, item.operator)) {
        editAsText(key, getEmptyText(term, item.operator));
        return;
      }

      editor?.commands.updateQueryToken({
        key,
        term: setOperator(term, item.operator),
      });
      setEditing(null);
      return;
    }
    if (item.kind === 'field') {
      const range = isPicking ? undefined : caret;
      editor?.commands.insertFilterToken(
        { key: item.field.key, values: [], negate: item.negate },
        range
      );
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

    setActiveKey(item.key);
    if (!field.multiple) {
      editor?.commands.updateFilterToken({
        key: field.key,
        values: [item.value],
        negate: false,
      });
      commitToken({ isOpen: false });
      return;
    }

    editor?.commands.clearFilterSearch();
    editor?.commands.updateFilterToken({
      key: field.key,
      negate: value.negate,
      values: value.values.includes(item.value)
        ? value.values.filter((entry) => entry !== item.value)
        : [...value.values, item.value],
    });
  }

  function moveAcross(token: FilterTokenAttrs | null, step: number) {
    if (!editing) {
      return;
    }

    const key = editing.key;
    stopEditing();
    if (token) {
      startEditing({ key: token.key, part: 'values' });
      return;
    }

    editor
      ?.chain()
      .placeFilterCaret(key, step < 0 ? 'before' : 'after')
      .scrollIntoView()
      .focus()
      .run();
    setIsOpen(true);
  }

  // Text input reaches React after the next key can arrive, so keys read the doc from the editor.
  function handleKeyDown(event: KeyboardEvent, state: EditorState) {
    const doc = readDoc(state.doc);
    const editedToken = doc.tokens.find((token) => token.key === editing?.key);
    const { from, empty } = state.selection;
    const $from = state.doc.resolve(from);

    if (
      event.key.length === 1 ||
      event.key === 'Backspace' ||
      event.key === 'Delete'
    ) {
      markTyping();
    }

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
        if (!editor || !empty || event.altKey || event.metaKey) {
          return false;
        }
        if (queryToken) {
          setEditing(null);
          return false;
        }

        const step = event.key === 'ArrowLeft' ? -1 : 1;
        const node = step < 0 ? $from.nodeBefore : $from.nodeAfter;

        // The editor moves the caret itself, so the next key sees where it is.
        if (editing) {
          if (isFilterSearch(node)) {
            editor.chain().setTextSelection(from + step).scrollIntoView().run();
            return true;
          }

          const index = doc.tokens.findIndex(
            (token) => token.key === editing.key
          );
          const beside =
            step < 0
              ? state.doc.resolve(from - 1).nodeBefore
              : $from.nodeAfter;
          moveAcross(
            isFilterToken(beside) ? doc.tokens[index + step]! : null,
            step
          );
          return true;
        }

        if (isFilterToken(node)) {
          startEditing({
            key: (node!.attrs as FilterTokenAttrs).key,
            part: 'values',
          });
        } else if (node) {
          editor.chain().setTextSelection(from + step).scrollIntoView().run();
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
          setIsPicking(false);
          return true;
        }
        if (queryToken) {
          setEditing(null);
          return true;
        }

        if (editing.snapshot) {
          editor
            ?.chain()
            .clearFilterSearch()
            .updateFilterToken({ key: editing.key, ...editing.snapshot })
            .run();
        } else {
          editor
            ?.chain()
            .clearFilterSearch()
            .removeFilterToken(editing.key)
            .run();
        }
        setEditing(null);
        return true;
      case ' ':
        if (!editing || doc.search || !editedToken?.values.length) {
          return false;
        }
        commitToken({ isOpen: true });
        return true;
      case 'Backspace': {
        if (event.metaKey) {
          editor?.commands.deleteRange({ from: 0, to: state.selection.to });
          setEditing(null);
          setIsOpen(true);
          return true;
        }

        if (!empty) {
          return false;
        }

        if (editing) {
          if (isFilterSearch($from.nodeBefore)) {
            return false;
          }
          editor
            ?.chain()
            .clearFilterSearch()
            .removeFilterToken(editing.key)
            .run();
          setEditing(null);
          setIsOpen(true);
          return true;
        }

        if (isFilterToken($from.nodeBefore)) {
          startEditing({
            key: ($from.nodeBefore!.attrs as FilterTokenAttrs).key,
            part: 'values',
          });
          return true;
        }
        return false;
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
        onEditText: editAsText,
        onRemove: (key) => {
          editor?.commands.removeFilterToken(key);
          if (key === editing?.key) {
            editor?.commands.clearFilterSearch();
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
        <Icon
          name={isSearching ? 'loader' : 'search'}
          className={cn('shrink-0 text-text-tertiary', isSearching && 'animate-spin')}
        />

        <div className="relative flex min-w-0 flex-1 items-center">
          <EditorContent editor={editor} className="flex min-w-0 flex-1" />
          {tokens.length === 0 && !docQuery && !search && (
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
            operator={operator}
            error={error}
            hint={hint}
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
              setIsPicking(false);
            }}
          />
        )}
      </div>
    </TokenContext.Provider>
  );
}
