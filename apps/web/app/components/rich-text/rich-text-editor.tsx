import type { Editor, JSONContent } from '@tiptap/core';
import { Placeholder } from '@tiptap/extensions';
import { EditorContent, useEditor, useEditorState } from '@tiptap/react';
import { StarterKit } from '@tiptap/starter-kit';
import { useState } from 'react';
import type { ReactNode } from 'react';

import { Icon } from '~/components/icons/icon';
import type { IconName } from '~/components/icons/icon-paths';
import { RICH_TEXT_CLASS_NAME } from '~/components/rich-text/rich-text';
import { Button } from '~/components/ui/button';
import { Input } from '~/components/ui/input';
import { Menu } from '~/components/ui/menu';
import { Popover } from '~/components/ui/popover';
import { cn } from '~/utils/classname';
import { isSafeHref } from '~/utils/rich-text';

type MarkButton = {
  icon: IconName;
  label: string;
  isActive: (editor: Editor) => boolean;
  run: (editor: Editor) => void;
};

const MARK_BUTTONS: MarkButton[] = [
  {
    icon: 'bold',
    label: 'Bold',
    isActive: (editor) => editor.isActive('bold'),
    run: (editor) => editor.chain().focus().toggleBold().run(),
  },
  {
    icon: 'italic',
    label: 'Italic',
    isActive: (editor) => editor.isActive('italic'),
    run: (editor) => editor.chain().focus().toggleItalic().run(),
  },
  {
    icon: 'strikethrough',
    label: 'Strikethrough',
    isActive: (editor) => editor.isActive('strike'),
    run: (editor) => editor.chain().focus().toggleStrike().run(),
  },
  {
    icon: 'code',
    label: 'Code',
    isActive: (editor) => editor.isActive('code'),
    run: (editor) => editor.chain().focus().toggleCode().run(),
  },
];

const BLOCK_BUTTONS: MarkButton[] = [
  {
    icon: 'list',
    label: 'Bulleted list',
    isActive: (editor) => editor.isActive('bulletList'),
    run: (editor) => editor.chain().focus().toggleBulletList().run(),
  },
  {
    icon: 'list-ordered',
    label: 'Numbered list',
    isActive: (editor) => editor.isActive('orderedList'),
    run: (editor) => editor.chain().focus().toggleOrderedList().run(),
  },
  {
    icon: 'quote',
    label: 'Quote',
    isActive: (editor) => editor.isActive('blockquote'),
    run: (editor) => editor.chain().focus().toggleBlockquote().run(),
  },
  {
    icon: 'braces',
    label: 'Code block',
    isActive: (editor) => editor.isActive('codeBlock'),
    run: (editor) => editor.chain().focus().toggleCodeBlock().run(),
  },
];

const TEXT_STYLES = [
  { value: 'text', label: 'Text', icon: 'type' },
  { value: 'h2', label: 'Heading 2', icon: 'heading-2' },
  { value: 'h3', label: 'Heading 3', icon: 'heading-3' },
] as const;

type TextStyle = (typeof TEXT_STYLES)[number]['value'];

function readTextStyle(editor: Editor): TextStyle {
  if (editor.isActive('heading', { level: 2 })) {
    return 'h2';
  }
  if (editor.isActive('heading', { level: 3 })) {
    return 'h3';
  }
  return 'text';
}

function applyTextStyle(editor: Editor, style: TextStyle) {
  const chain = editor.chain().focus();
  if (style === 'text') {
    chain.setParagraph().run();
  } else {
    chain.setHeading({ level: style === 'h2' ? 2 : 3 }).run();
  }
}

type RichTextEditorProps = {
  content?: JSONContent | null;
  onChange: (content: JSONContent) => void;
  placeholder: string;
  label: string;
  hasHeadings?: boolean;
  autoFocus?: boolean;
  className?: string;
  toolbarClassName?: string;
  contentClassName?: string;
  toolbarEnd?: ReactNode;
};

export function RichTextEditor(props: RichTextEditorProps) {
  const {
    content,
    onChange,
    placeholder,
    label,
    hasHeadings = false,
    autoFocus = false,
    className,
    toolbarClassName,
    contentClassName,
    toolbarEnd,
  } = props;

  const editor = useEditor({
    immediatelyRender: false,
    autofocus: autoFocus ? 'end' : false,
    extensions: [
      StarterKit.configure({
        heading: hasHeadings ? { levels: [2, 3] } : false,
        underline: false,
        link: {
          openOnClick: false,
          autolink: true,
          defaultProtocol: 'https',
          isAllowedUri: (url) => isSafeHref(url),
        },
      }),
      Placeholder.configure({ placeholder }),
    ],
    content: content ?? undefined,
    editorProps: {
      attributes: {
        'aria-label': label,
        'aria-multiline': 'true',
        role: 'textbox',
        class: 'outline-none',
      },
    },
    onUpdate: ({ editor }) => onChange(editor.getJSON()),
  });

  return (
    <div className={cn('flex min-w-0 flex-col', className)}>
      <div
        role="toolbar"
        aria-label={`${label} formatting`}
        className={cn('flex items-center gap-0.5 overflow-x-auto border-b border-border-ring p-1.5', toolbarClassName)}
      >
        {editor && <Toolbar editor={editor} hasHeadings={hasHeadings} />}
        {toolbarEnd && <div className="ml-auto flex shrink-0 items-center">{toolbarEnd}</div>}
      </div>
      <EditorContent
        editor={editor}
        className={cn(
          RICH_TEXT_CLASS_NAME,
          'min-h-16 cursor-text px-3 py-3 text-text-primary [&_.ProseMirror]:min-h-[inherit] [&_.ProseMirror]:outline-none',
          '[&_.is-editor-empty:first-child]:before:pointer-events-none [&_.is-editor-empty:first-child]:before:float-left [&_.is-editor-empty:first-child]:before:h-0 [&_.is-editor-empty:first-child]:before:text-text-tertiary [&_.is-editor-empty:first-child]:before:content-[attr(data-placeholder)]',
          contentClassName
        )}
        onClick={() => editor?.chain().focus().run()}
      />
    </div>
  );
}

type ToolbarProps = {
  editor: Editor;
  hasHeadings: boolean;
};

function Toolbar(props: ToolbarProps) {
  const { editor, hasHeadings } = props;

  const state = useEditorState({
    editor,
    selector: ({ editor }) => ({
      active: [...MARK_BUTTONS, ...BLOCK_BUTTONS].map((button) => button.isActive(editor)),
      isLink: editor.isActive('link'),
      textStyle: readTextStyle(editor),
    }),
  });

  const currentStyle = TEXT_STYLES.find((style) => style.value === state.textStyle) ?? TEXT_STYLES[0];

  return (
    <>
      {hasHeadings && (
        <>
          <Menu.Root>
            <Menu.Trigger
              aria-label={`Text style: ${currentStyle.label}`}
              className="flex h-7 shrink-0 items-center gap-1 rounded-[10px] pr-1.5 pl-2 text-body font-medium text-text-primary outline-none hover:bg-surface-fill focus-visible:ring-2 focus-visible:ring-text-link/40 data-popup-open:bg-surface-fill"
            >
              {currentStyle.label}
              <Icon name="chevron-down" className="size-3.5 text-text-tertiary" />
            </Menu.Trigger>
            <Menu.Portal>
              <Menu.Positioner align="start" sideOffset={6}>
                <Menu.Popup className="min-w-40">
                  <Menu.RadioGroup
                    value={state.textStyle}
                    onValueChange={(value) => applyTextStyle(editor, value as TextStyle)}
                  >
                    {TEXT_STYLES.map((style) => (
                      <Menu.RadioItem key={style.value} value={style.value}>
                        <Icon name={style.icon} />
                        {style.label}
                      </Menu.RadioItem>
                    ))}
                  </Menu.RadioGroup>
                </Menu.Popup>
              </Menu.Positioner>
            </Menu.Portal>
          </Menu.Root>
          <ToolbarDivider />
        </>
      )}
      {MARK_BUTTONS.map((button, index) => (
        <ToolbarButton
          key={button.label}
          icon={button.icon}
          label={button.label}
          isActive={state.active[index] ?? false}
          onClick={() => button.run(editor)}
        />
      ))}
      <LinkButton editor={editor} isActive={state.isLink} />
      <ToolbarDivider />
      {BLOCK_BUTTONS.map((button, index) => (
        <ToolbarButton
          key={button.label}
          icon={button.icon}
          label={button.label}
          isActive={state.active[MARK_BUTTONS.length + index] ?? false}
          onClick={() => button.run(editor)}
        />
      ))}
    </>
  );
}

function ToolbarDivider() {
  return <span aria-hidden="true" className="mx-1 h-4 w-px shrink-0 bg-border-ring" />;
}

type ToolbarButtonProps = {
  icon: IconName;
  label: string;
  isActive: boolean;
  onClick?: () => void;
};

const TOOLBAR_BUTTON_CLASS_NAME =
  'flex size-7 shrink-0 items-center justify-center rounded-[10px] text-text-tertiary outline-none hover:bg-surface-fill hover:text-text-primary focus-visible:ring-2 focus-visible:ring-text-link/40 aria-pressed:bg-surface-fill-strong aria-pressed:text-text-primary data-popup-open:bg-surface-fill [&_svg]:size-3.5';

function ToolbarButton(props: ToolbarButtonProps) {
  const { icon, label, isActive, onClick } = props;

  return (
    <button
      type="button"
      title={label}
      aria-label={label}
      aria-pressed={isActive}
      onMouseDown={(event) => event.preventDefault()}
      onClick={onClick}
      className={TOOLBAR_BUTTON_CLASS_NAME}
    >
      <Icon name={icon} />
    </button>
  );
}

type LinkButtonProps = {
  editor: Editor;
  isActive: boolean;
};

function LinkButton(props: LinkButtonProps) {
  const { editor, isActive } = props;

  const [open, setOpen] = useState(false);
  const [href, setHref] = useState('');
  const normalized = /^[a-z]+:/i.test(href.trim()) ? href.trim() : `https://${href.trim()}`;
  const isValid = href.trim().length > 0 && isSafeHref(normalized);

  const save = () => {
    if (!isValid) {
      return;
    }
    editor.chain().focus().extendMarkRange('link').setLink({ href: normalized }).run();
    setOpen(false);
  };

  return (
    <Popover.Root
      open={open}
      onOpenChange={(next) => {
        setOpen(next);
        if (next) {
          setHref(String(editor.getAttributes('link').href ?? ''));
        }
      }}
    >
      <Popover.Trigger
        title="Link"
        aria-label="Link"
        aria-pressed={isActive}
        onMouseDown={(event) => event.preventDefault()}
        className={TOOLBAR_BUTTON_CLASS_NAME}
      >
        <Icon name="link" />
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Positioner align="start" sideOffset={6}>
          <Popover.Popup className="w-80 p-2">
            <form
              className="flex items-center gap-1.5"
              onSubmit={(event) => {
                event.preventDefault();
                save();
              }}
            >
              <Input
                autoFocus
                aria-label="Link address"
                placeholder="https://"
                value={href}
                onChange={(event) => setHref(event.target.value)}
                className="flex-1"
              />
              {isActive && (
                <Button
                  type="button"
                  variant="secondary"
                  aria-label="Remove link"
                  onClick={() => {
                    editor.chain().focus().extendMarkRange('link').unsetLink().run();
                    setOpen(false);
                  }}
                >
                  <Icon name="trash" />
                </Button>
              )}
              <Button type="submit" disabled={!isValid}>
                Save
              </Button>
            </form>
          </Popover.Popup>
        </Popover.Positioner>
      </Popover.Portal>
    </Popover.Root>
  );
}
