import type { ReactNode } from 'react';

type DropdownNoteProps = {
  children: ReactNode;
};

export function DropdownNote(props: DropdownNoteProps) {
  const { children } = props;

  return (
    <p className="px-2 py-1.5 text-small text-text-tertiary">{children}</p>
  );
}
