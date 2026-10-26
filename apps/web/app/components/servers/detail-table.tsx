import type { ReactNode } from 'react';

type DetailSectionProps = {
  title: string;
  summary?: string;
  children: ReactNode;
};

export function DetailSection(props: DetailSectionProps) {
  const { title, summary, children } = props;

  return (
    <section className="flex flex-col gap-3">
      <div className="flex items-center">
        <h2 className="flex-1 text-title-section font-medium text-text-primary">{title}</h2>
        {summary && <span className="text-body text-text-secondary">{summary}</span>}
      </div>
      {children}
    </section>
  );
}

type DetailCardProps = {
  children: ReactNode;
};

export function DetailCard(props: DetailCardProps) {
  const { children } = props;

  return (
    <div className="overflow-hidden rounded-2xl border border-border-ring bg-surface-default pb-1 shadow-card">
      {children}
    </div>
  );
}

type DetailEmptyProps = {
  title?: string;
  children: ReactNode;
};

export function DetailEmpty(props: DetailEmptyProps) {
  const { title, children } = props;

  if (!title) {
    return <p className="px-4 py-3 text-body text-text-secondary">{children}</p>;
  }

  return (
    <div className="flex flex-col items-center gap-0.5 px-4 py-6 text-center">
      <p className="text-body font-medium text-text-primary">{title}</p>
      <p className="text-body text-text-secondary">{children}</p>
    </div>
  );
}
