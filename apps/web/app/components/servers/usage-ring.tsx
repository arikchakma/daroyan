import { cva } from 'class-variance-authority';

export type UsageTone = 'ok' | 'warn' | 'crit';

const ring = cva('', {
  variants: {
    tone: {
      ok: 'stroke-chart-primary',
      warn: 'stroke-toast-warning-icon',
      crit: 'stroke-delta-bad',
    },
  },
});

type UsageRingProps = {
  percent: number;
  tone: UsageTone;
};

export function UsageRing(props: UsageRingProps) {
  const { percent, tone } = props;

  const clamped = Math.max(0, Math.min(percent, 100));

  return (
    <svg viewBox="0 0 44 44" className="size-7 shrink-0 -rotate-90" aria-hidden="true">
      <circle cx="22" cy="22" r="18" fill="none" strokeWidth="5" className="stroke-surface-fill-strong" />
      <circle
        cx="22"
        cy="22"
        r="18"
        fill="none"
        strokeWidth="5"
        strokeLinecap="round"
        pathLength={100}
        strokeDasharray={`${clamped} 100`}
        className={ring({ tone })}
      />
    </svg>
  );
}
