import type { JSONContent } from '@tiptap/core';
import { Fragment } from 'react';
import type { ReactNode } from 'react';

import { cn } from '~/utils/classname';

export type RichTextSize = 'body' | 'prose';

export type RichTextNodeClassNames = {
  paragraph: string;
  heading2: string;
  heading3: string;
  bulletList: string;
  orderedList: string;
  listItem: string;
  blockquote: string;
  codeBlock: string;
  horizontalRule: string;
  bold: string;
  code: string;
  link: string;
};

type NodeRenderer = (node: JSONContent, classNames: RichTextNodeClassNames) => ReactNode;
type MarkRenderer = (
  mark: NonNullable<JSONContent['marks']>[number],
  text: ReactNode,
  classNames: RichTextNodeClassNames
) => ReactNode;

const LINK_PROTOCOLS = ['https://', 'http://', 'mailto:'];
const MARKS_ORDER = ['bold', 'italic', 'strike', 'link', 'code'];

const BLOCK_CLASS_NAME = 'mt-3 first:mt-0 [h2+&]:mt-2 [h3+&]:mt-1';
const LIST_CLASS_NAME = cn(BLOCK_CLASS_NAME, 'pl-5.5 marker:text-text-tertiary [li>&]:mt-1');

export function getRichTextClassName(size: RichTextSize) {
  if (size === 'prose') {
    return 'min-w-0 text-prose text-text-primary [overflow-wrap:anywhere]';
  }

  return 'min-w-0 text-body text-text-secondary [overflow-wrap:anywhere]';
}

export function getRichTextNodeClassNames(size: RichTextSize): RichTextNodeClassNames {
  const isProse = size === 'prose';

  return {
    paragraph: cn(BLOCK_CLASS_NAME, 'min-h-[1lh]'),
    heading2: cn(
      'mt-6 font-medium text-text-primary first:mt-0',
      isProse ? 'text-stat' : 'text-title-section'
    ),
    heading3: cn(
      'mt-5 font-semibold text-text-primary first:mt-0 [h2+&]:mt-2',
      isProse ? 'text-title-section leading-6' : 'text-body'
    ),
    bulletList: cn(LIST_CLASS_NAME, 'list-disc'),
    orderedList: cn(LIST_CLASS_NAME, 'list-decimal'),
    listItem: 'mt-1 pl-0.5 first:mt-0',
    blockquote: cn(BLOCK_CLASS_NAME, 'border-l-2 border-border-default pl-3.5 text-text-secondary'),
    codeBlock: cn(
      BLOCK_CLASS_NAME,
      'overflow-x-auto rounded-lg bg-surface-fill px-3.5 py-3 font-mono tracking-normal text-text-primary',
      isProse ? 'text-[13px]/5' : 'text-code'
    ),
    horizontalRule: cn(BLOCK_CLASS_NAME, 'border-border-row'),
    bold: 'font-semibold text-text-primary',
    code: 'font-mono text-[0.875em] tracking-normal text-tone-magic-fg',
    link: 'text-text-link underline underline-offset-2',
  };
}

const NODE_RENDERERS: Record<string, NodeRenderer> = {
  doc: (node, classNames) => <>{children(node, classNames)}</>,
  paragraph: (node, classNames) => <p className={classNames.paragraph}>{children(node, classNames)}</p>,
  heading: (node, classNames) => {
    if (node.attrs?.level === 3) {
      return <h3 className={classNames.heading3}>{children(node, classNames)}</h3>;
    }

    return <h2 className={classNames.heading2}>{children(node, classNames)}</h2>;
  },
  bulletList: (node, classNames) => <ul className={classNames.bulletList}>{children(node, classNames)}</ul>,
  orderedList: (node, classNames) => (
    <ol start={node.attrs?.start ?? undefined} className={classNames.orderedList}>
      {children(node, classNames)}
    </ol>
  ),
  listItem: (node, classNames) => <li className={classNames.listItem}>{children(node, classNames)}</li>,
  blockquote: (node, classNames) => (
    <blockquote className={classNames.blockquote}>{children(node, classNames)}</blockquote>
  ),
  codeBlock: (node, classNames) => (
    <pre className={classNames.codeBlock}>
      <code>{children(node, classNames)}</code>
    </pre>
  ),
  hardBreak: () => <br />,
  horizontalRule: (_, classNames) => <hr className={classNames.horizontalRule} />,
  text: (node, classNames) => marks(node, classNames),
};

const MARK_RENDERERS: Record<string, MarkRenderer> = {
  bold: (_, text, classNames) => <strong className={classNames.bold}>{text}</strong>,
  italic: (_, text) => <em>{text}</em>,
  strike: (_, text) => <s>{text}</s>,
  code: (_, text, classNames) => <code className={classNames.code}>{text}</code>,
  link: (mark, text, classNames) => {
    const href = String(mark.attrs?.href ?? '');
    if (!LINK_PROTOCOLS.some((protocol) => href.toLowerCase().startsWith(protocol))) {
      return text;
    }
    return (
      <a href={href} target="_blank" rel="noopener noreferrer nofollow" className={classNames.link}>
        {text}
      </a>
    );
  },
};

function dispatch(node: JSONContent, classNames: RichTextNodeClassNames): ReactNode {
  const renderer = node.type ? NODE_RENDERERS[node.type] : undefined;
  return renderer ? renderer(node, classNames) : null;
}

function children(parent: JSONContent, classNames: RichTextNodeClassNames): ReactNode[] {
  return (parent.content ?? []).map((child, index) => (
    <Fragment key={index}>{dispatch(child, classNames)}</Fragment>
  ));
}

function marks(node: JSONContent, classNames: RichTextNodeClassNames): ReactNode {
  const sorted = [...(node.marks ?? [])].sort(
    (a, b) => MARKS_ORDER.indexOf(a.type) - MARKS_ORDER.indexOf(b.type)
  );
  return sorted.reduce<ReactNode>((text, mark) => {
    const renderer = MARK_RENDERERS[mark.type];
    return renderer ? renderer(mark, text, classNames) : text;
  }, node.text ?? '');
}

type RichTextProps = {
  content: JSONContent | null | undefined;
  size?: RichTextSize;
  className?: string;
};

export function RichText(props: RichTextProps) {
  const { content, size = 'body', className } = props;

  if (!content || content.type !== 'doc') {
    return null;
  }

  return (
    <div className={cn(getRichTextClassName(size), className)}>
      {dispatch(content, getRichTextNodeClassNames(size))}
    </div>
  );
}
