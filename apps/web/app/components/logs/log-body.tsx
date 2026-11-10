import type { ReactNode } from 'react';

import { LogContextList } from '~/components/logs/log-context-list';
import { LogDetails } from '~/components/logs/log-details';
import { useFullLogLine } from '~/hooks/use-full-log-line';
import { describeRequest, LevelBadge } from '~/hooks/use-logs-table';
import type { LogRow } from '~/hooks/use-logs-table';
import { getLogOrigin } from '~/utils/logs';

type LogBodyProps = {
  line: LogRow;
};

export function LogBody(props: LogBodyProps) {
  const { line: row } = props;

  const { context, isLoading, full: line } = useFullLogLine(row);
  const request = describeRequest(line);

  return (
    <div className="flex pb-16 max-lg:flex-col">
      <div className="flex min-w-0 flex-1 flex-col gap-6 px-10 pt-6 max-lg:px-4">
        <div className="flex flex-wrap items-center gap-3">
          <LevelBadge level={line.level} />
          <p className="text-body text-text-secondary">{getLogOrigin(line)}</p>
        </div>

        <LogSection title="Message">
          <pre className="max-h-80 overflow-auto rounded-xl bg-surface-code px-4 py-3 font-mono text-code-small wrap-anywhere whitespace-pre-wrap text-white">
            {request ? `${request} · ${line.message}` : line.message}
          </pre>
        </LogSection>

        {line.raw && (
          <LogSection title="Raw Line" caption={`Exactly as ${line.service} wrote it, before parsing.`}>
            <pre className="max-h-60 overflow-auto rounded-xl border border-border-ring bg-surface-default px-4 py-3 font-mono text-code-small wrap-anywhere whitespace-pre-wrap shadow-card">
              {line.raw}
            </pre>
          </LogSection>
        )}

        <LogSection title="Before and After" caption={`What ${line.service} wrote just before and after this line.`}>
          <LogContextList line={line} context={context} isLoading={isLoading} />
        </LogSection>
      </div>

      <LogDetails line={line} />
    </div>
  );
}

type LogSectionProps = {
  title: string;
  caption?: string;
  children: ReactNode;
};

function LogSection(props: LogSectionProps) {
  const { title, caption, children } = props;

  return (
    <section className="flex flex-col gap-2.5">
      <div className="flex flex-col gap-0.5">
        <h2 className="text-title-section font-medium">{title}</h2>
        {caption && <p className="text-body text-text-secondary">{caption}</p>}
      </div>
      {children}
    </section>
  );
}
