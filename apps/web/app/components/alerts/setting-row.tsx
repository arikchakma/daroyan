import { useId } from 'react';
import type { ReactNode } from 'react';

import { Badge } from '~/components/ui/badge';
import { cn } from '~/utils/classname';

type SettingRowProps = {
  title: string;
  description: string;
  isCaption?: boolean;
  isChanged?: boolean;
  className?: string;
  control?: (labelId: string) => ReactNode;
  actions?: ReactNode | ((labelId: string) => ReactNode);
  children?: ReactNode;
};

export function SettingRow(props: SettingRowProps) {
  const { title, description, isCaption, isChanged, className, control, actions, children } = props;

  const labelId = useId();

  return (
    <div className={cn('flex flex-col gap-3.5 px-3.5 py-3', children && 'pb-3.5', className)}>
      <div className="flex items-center gap-4">
        <div className="flex min-w-0 flex-1 flex-col gap-0.5">
          <span className="flex items-center gap-2">
            <span id={labelId} className="text-body font-medium">
              {title}
            </span>
            {isChanged && <Badge tone="info">Changed</Badge>}
          </span>
          <span className={cn('text-text-secondary', isCaption ? 'text-caption leading-4.5' : 'text-body')}>{description}</span>
        </div>
        {control && <div className="w-60 shrink-0">{control(labelId)}</div>}
        {typeof actions === 'function' ? actions(labelId) : actions}
      </div>
      {children}
    </div>
  );
}
