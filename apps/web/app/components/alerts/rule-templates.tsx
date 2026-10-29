import { TEMPLATES } from '~/utils/rule-form';
import type { RuleTemplate } from '~/utils/rule-form';

type RuleTemplatesProps = {
  onApply: (template: RuleTemplate) => void;
};

export function RuleTemplates(props: RuleTemplatesProps) {
  const { onApply } = props;

  return (
    <>
      <div className="h-px bg-border-row" />
      <div className="flex flex-col gap-2.5">
        <h2 className="text-body font-medium">Start From a Template</h2>
        <div className="flex flex-wrap gap-1.5">
          {TEMPLATES.map((template) => (
            <button
              key={template.label}
              type="button"
              onClick={() => onApply(template)}
              className="flex h-5.5 items-center rounded-xs bg-surface-fill px-1.75 text-caption font-normal text-text-secondary outline-none hover:bg-surface-fill-strong hover:text-text-primary focus-visible:ring-2 focus-visible:ring-text-link/40"
            >
              {template.label}
            </button>
          ))}
        </div>
      </div>
    </>
  );
}
