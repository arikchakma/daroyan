import type { JSONContent } from '@tiptap/core';

const LINK_PROTOCOLS = ['https://', 'http://', 'mailto:'];

export function isSafeHref(href: string) {
  const value = href.trim().toLowerCase();
  return LINK_PROTOCOLS.some((protocol) => value.startsWith(protocol) && value.length > protocol.length);
}

export function toPlainText(node: JSONContent | null | undefined): string {
  if (!node) {
    return '';
  }
  if (node.type === 'text') {
    return node.text ?? '';
  }
  const text = (node.content ?? []).map(toPlainText).join(node.type === 'doc' ? '\n' : '');
  return text;
}

export function hasText(node: JSONContent | null | undefined) {
  return toPlainText(node).trim().length > 0;
}

export function toDocument(value: unknown): JSONContent | null {
  if (value && typeof value === 'object' && (value as JSONContent).type === 'doc') {
    return value as JSONContent;
  }
  return null;
}
