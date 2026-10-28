import { Checkbox } from '~/components/ui/checkbox';

export type Target = {
  id: string;
  name: string;
  detail: string;
  labels: string[];
};

type PickedTargetsProps = {
  noun: string;
  targets: Target[];
  pickedIds: string[];
  onPickedIdsChange: (pickedIds: string[]) => void;
};

export function PickedTargets(props: PickedTargetsProps) {
  const { noun, targets, pickedIds, onPickedIdsChange } = props;

  return (
    <div className="flex flex-col py-1">
      {targets.length === 0 && <p className="px-3.5 py-2.5 text-body text-text-secondary">No {noun}s yet.</p>}
      {targets.map((target) => (
        <label key={target.id} className="flex cursor-pointer items-center gap-3 px-3.5 py-2.5 hover:bg-surface-subtle">
          <Checkbox
            checked={pickedIds.includes(target.id)}
            onCheckedChange={(checked) =>
              onPickedIdsChange(checked ? [...pickedIds, target.id] : pickedIds.filter((id) => id !== target.id))
            }
          />
          <span className="flex-1 text-body">{target.name}</span>
          <span className="truncate text-caption font-normal text-text-tertiary">{target.detail}</span>
        </label>
      ))}
    </div>
  );
}
