const BAR_HEIGHTS = Array.from({ length: 96 }, (_, index) => 45 + 20 * Math.sin(index / 9) + 8 * Math.sin(index / 2.3));
const AXIS_LABELS = 8;

export function LogVolumeSkeleton() {
  return (
    <div aria-hidden className="flex h-21 animate-pulse flex-col gap-1.5">
      <div className="flex flex-1 items-end gap-0.5">
        {BAR_HEIGHTS.map((height, index) => (
          <span key={index} className="flex-1 rounded-t-[2px] bg-surface-fill-strong" style={{ height: `${height}%` }} />
        ))}
      </div>
      <div className="flex h-4 justify-between">
        {Array.from({ length: AXIS_LABELS }, (_, index) => (
          <span key={index} className="my-1 w-8 rounded-full bg-surface-fill-strong" />
        ))}
      </div>
    </div>
  );
}
