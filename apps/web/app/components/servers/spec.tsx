import { Sparkline } from '~/components/servers/sparkline';
import { UsageRing } from '~/components/servers/usage-ring';
import type { UsageTone } from '~/components/servers/usage-ring';
import { cn } from '~/utils/classname';

export type SpecProps = {
  label: string;
  value: string;
  detail?: string;
  ring?: { percent: number; tone: UsageTone };
  spark?: number[];
  valueCritical?: boolean;
};

export function Spec(props: SpecProps) {
  const { label, value, detail, ring, spark, valueCritical } = props;

  return (
    <div className="flex min-w-0 items-center justify-between gap-3 bg-surface-default px-4 py-3.5">
      <div className="flex min-w-0 flex-col gap-1">
        <span className="text-caption font-medium text-text-secondary">{label}</span>
        <div className="flex min-w-0 items-center gap-1.5 whitespace-nowrap">
          <span className={cn('min-w-0 truncate text-body font-semibold', valueCritical ? 'text-tone-critical-fg' : 'text-text-primary')}>
            {value}
          </span>
          {detail && <span className="min-w-0 shrink-1000 truncate text-caption font-normal text-text-tertiary">{detail}</span>}
        </div>
      </div>
      {ring && <UsageRing percent={ring.percent} tone={ring.tone} />}
      {spark && <Sparkline values={spark} />}
    </div>
  );
}
