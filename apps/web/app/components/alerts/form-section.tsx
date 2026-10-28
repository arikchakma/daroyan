import type { ReactNode } from 'react';

type FormSectionProps = {
  title: string;
  description?: string;
  action?: ReactNode;
  children: ReactNode;
};

export function FormSection(props: FormSectionProps) {
  const { title, description, action, children } = props;

  return (
    <section className="flex flex-col gap-3">
      <div className="flex items-center gap-3">
        <div className="flex min-w-0 flex-1 flex-col gap-0.5">
          <h2 className="text-title-section font-medium">{title}</h2>
          {description && <p className="text-body text-text-secondary">{description}</p>}
        </div>
        {action}
      </div>
      <div className="flex flex-col overflow-hidden rounded-2xl border border-border-ring bg-surface-default shadow-card [&>*+*]:border-t [&>*+*]:border-border-row">
        {children}
      </div>
    </section>
  );
}
