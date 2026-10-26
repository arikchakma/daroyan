import type { ReactNode } from 'react';
import { useId } from 'react';

type FieldRowProps = {
  label: string;
  hint?: string;
  children: (ids: { inputId: string; hintId: string }) => ReactNode;
};

export function FieldRow(props: FieldRowProps) {
  const { label, hint, children } = props;

  const inputId = useId();
  const hintId = useId();

  return (
    <div className="flex flex-col gap-1.5">
      <label htmlFor={inputId} className="text-body font-medium">
        {label}
      </label>
      {children({ inputId, hintId })}
      {hint && (
        <p id={hintId} className="text-caption text-text-secondary">
          {hint}
        </p>
      )}
    </div>
  );
}
