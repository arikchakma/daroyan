import type { Schemas } from '@daroyan/client';

import { Icon } from '~/components/icons/icon';
import { DetailSection } from '~/components/servers/detail-table';
import { CodeBlock } from '~/components/ui/code-block';

type FirstPingProps = {
  cron: Schemas['Cron'];
  pingUrl: string;
};

export function FirstPing(props: FirstPingProps) {
  const { cron, pingUrl } = props;

  const schedule = cron.schedule_kind === 'cron' ? cron.expression : '0 * * * *';
  const command = cron.command ?? 'cmd';

  return (
    <DetailSection title="Send the First Ping">
      <p className="-mt-2 text-body text-text-secondary">Add one of these to the job. Nothing shows here until it pings.</p>
      <div className="flex flex-col overflow-hidden rounded-2xl border border-border-ring bg-surface-default shadow-card [&>*+*]:border-t [&>*+*]:border-border-row">
        <SetupBlock
          title="Ping URL"
          description="Call it when the job finishes. Add /start when it begins, and /fail or the exit code when it ends."
          code={pingUrl}
        />
        <SetupBlock
          title="Crontab"
          description="Put curl after your command, so the ping only goes out when the job succeeds."
          code={`${schedule} ${command} && curl -fsS ${pingUrl}`}
        />
        <SetupBlock
          title="With the Exit Code and Output"
          description="Sends /start, then the exit code with up to 10 kB of output, so failures show what went wrong."
          code={`curl -fsS ${pingUrl}/start; out=$(${command} 2>&1); curl -fsS --data-raw "$out" ${pingUrl}/$?`}
        />
        <div className="flex items-center gap-2 bg-surface-subtle px-4 py-3 text-body text-text-secondary">
          <Icon name="clock" />
          Daroyan starts watching after the first ping.
        </div>
      </div>
    </DetailSection>
  );
}

type SetupBlockProps = {
  title: string;
  description: string;
  code: string;
};

function SetupBlock(props: SetupBlockProps) {
  const { title, description, code } = props;

  return (
    <div className="flex flex-col gap-3 p-4">
      <div className="flex flex-col gap-0.5">
        <span className="text-body font-medium">{title}</span>
        <span className="text-body text-text-secondary">{description}</span>
      </div>
      <CodeBlock label={title} code={code} language="shellscript" />
    </div>
  );
}
