import type { ReactNode } from 'react';

type SettingsRowProps = {
  label: string;
  labelId?: string;
  envVar?: string;
  children: ReactNode;
};

export function SettingsRow(props: SettingsRowProps) {
  const { label, labelId, envVar, children } = props;

  return (
    <div className="flex min-h-11 items-center gap-4 px-4 py-3">
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span id={labelId} className="text-body">
          {label}
        </span>
        {envVar && <code className="font-mono text-caption text-text-tertiary">{envVar}</code>}
      </div>
      <div className="min-w-0 truncate text-body text-text-secondary">{children}</div>
    </div>
  );
}
