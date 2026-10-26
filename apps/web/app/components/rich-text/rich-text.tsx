import type { JSONContent } from '@tiptap/core';
import { Fragment } from 'react';
import type { ReactNode } from 'react';

import { cn } from '~/utils/classname';

type NodeRenderer = (node: JSONContent) => ReactNode;
type MarkRenderer = (mark: NonNullable<JSONContent['marks']>[number], text: ReactNode) => ReactNode;

const LINK_PROTOCOLS = ['https://', 'http://', 'mailto:'];
const MARKS_ORDER = ['bold', 'italic', 'strike', 'link', 'code'];

const NODE_RENDERERS: Record<string, NodeRenderer> = {
  doc: (node) => <>{children(node)}</>,
  paragraph: (node) => <p>{children(node)}</p>,
  heading: (node) => (node.attrs?.level === 3 ? <h3>{children(node)}</h3> : <h2>{children(node)}</h2>),
  bulletList: (node) => <ul>{children(node)}</ul>,
  orderedList: (node) => <ol start={node.attrs?.start ?? undefined}>{children(node)}</ol>,
  listItem: (node) => <li>{children(node)}</li>,
  blockquote: (node) => <blockquote>{children(node)}</blockquote>,
  codeBlock: (node) => (
    <pre>
      <code>{children(node)}</code>
    </pre>
  ),
  hardBreak: () => <br />,
  horizontalRule: () => <hr />,
  text: (node) => marks(node),
};

const MARK_RENDERERS: Record<string, MarkRenderer> = {
  bold: (_, text) => <strong>{text}</strong>,
  italic: (_, text) => <em>{text}</em>,
  strike: (_, text) => <s>{text}</s>,
  code: (_, text) => <code>{text}</code>,
  link: (mark, text) => {
    const href = String(mark.attrs?.href ?? '');
    if (!LINK_PROTOCOLS.some((protocol) => href.toLowerCase().startsWith(protocol))) {
      return text;
    }
    return (
      <a href={href} target="_blank" rel="noopener noreferrer nofollow">
        {text}
      </a>
    );
  },
};

function dispatch(node: JSONContent): ReactNode {
  const renderer = node.type ? NODE_RENDERERS[node.type] : undefined;
  return renderer ? renderer(node) : null;
}

function children(parent: JSONContent): ReactNode[] {
  return (parent.content ?? []).map((child, index) => <Fragment key={index}>{dispatch(child)}</Fragment>);
}

function marks(node: JSONContent): ReactNode {
  const sorted = [...(node.marks ?? [])].sort(
    (a, b) => MARKS_ORDER.indexOf(a.type) - MARKS_ORDER.indexOf(b.type)
  );
  return sorted.reduce<ReactNode>((text, mark) => {
    const renderer = MARK_RENDERERS[mark.type];
    return renderer ? renderer(mark, text) : text;
  }, node.text ?? '');
}

export const RICH_TEXT_CLASS_NAME = cn(
  'min-w-0 text-body text-text-secondary [overflow-wrap:anywhere]',
  '[&_p]:min-h-5 [&>*+*]:mt-1.5',
  '[&_h2]:text-title-section [&_h2]:font-medium [&_h2]:text-text-primary [&_h2:not(:first-child)]:mt-4',
  '[&_h3]:text-body [&_h3]:font-semibold [&_h3]:text-text-primary [&_h3:not(:first-child)]:mt-3',
  '[&_strong]:font-semibold [&_strong]:text-text-primary',
  '[&_a]:text-text-link [&_a]:underline [&_a]:underline-offset-2',
  '[&_ul]:list-disc [&_ol]:list-decimal [&_ul,&_ol]:pl-5 [&_li]:pl-0.5',
  '[&_blockquote]:border-l-2 [&_blockquote]:border-border-default [&_blockquote]:pl-3',
  '[&_code]:font-mono [&_code]:text-[0.94em] [&_code]:text-tone-magic-fg',
  '[&_pre]:overflow-x-auto [&_pre]:rounded-md [&_pre]:bg-surface-fill [&_pre]:px-3 [&_pre]:py-2 [&_pre_code]:text-text-primary',
  '[&_hr]:border-border-row'
);

type RichTextProps = {
  content: JSONContent | null | undefined;
  className?: string;
};

export function RichText(props: RichTextProps) {
  const { content, className } = props;

  if (!content || content.type !== 'doc') {
    return null;
  }

  return <div className={cn(RICH_TEXT_CLASS_NAME, className)}>{dispatch(content)}</div>;
}
