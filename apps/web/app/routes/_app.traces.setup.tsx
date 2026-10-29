import { useMutation, useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import type { ReactNode } from 'react';
import { Navigate } from 'react-router';

import { Icon } from '~/components/icons/icon';
import { Badge } from '~/components/ui/badge';
import { Button } from '~/components/ui/button';
import { CodeBlock } from '~/components/ui/code-block';
import { RailRow, RailSection } from '~/components/ui/detail-rail';
import { PageBar } from '~/components/ui/page-bar';
import { useIsAdmin } from '~/hooks/use-is-admin';
import { queryClient } from '~/lib/query-client';
import { toast } from '~/lib/toast';
import { createIngestTokenOptions, traceStatusOptions } from '~/queries/traces';

const POLL_MS = 3_000;

const LANGUAGES = [
  {
    value: 'node',
    label: 'Node.js',
    code: `npm install @opentelemetry/sdk-node \\
  @opentelemetry/auto-instrumentations-node
node --require @opentelemetry/auto-instrumentations-node/register app.js`,
  },
  {
    value: 'python',
    label: 'Python',
    code: `pip install opentelemetry-distro opentelemetry-exporter-otlp
opentelemetry-bootstrap -a install
opentelemetry-instrument python app.py`,
  },
  {
    value: 'go',
    label: 'Go',
    code: `go get go.opentelemetry.io/otel/sdk \\
  go.opentelemetry.io/otel/exporters/otlp/otlptrace/otlptracehttp
# otlptracehttp.New(ctx) reads the variables below`,
  },
  {
    value: 'rust',
    label: 'Rust',
    code: `cargo add opentelemetry opentelemetry_sdk
cargo add opentelemetry-otlp --features http-proto,reqwest-blocking-client
# SpanExporter::builder().with_http().build() reads the variables below`,
  },
] as const;

type Language = (typeof LANGUAGES)[number]['value'];

export function meta() {
  return [{ title: 'Set Up Traces · Daroyan' }];
}

export default function TraceSetupPage() {
  const [language, setLanguage] = useState<Language>('node');
  const [token, setToken] = useState<string>();
  const [openedAt] = useState(() => Date.now());
  const isAdmin = useIsAdmin();
  const { data: status } = useQuery(traceStatusOptions(POLL_MS));

  const create = useMutation({
    ...createIngestTokenOptions(),
    meta: { errorMessage: 'The token could not be created' },
    onSuccess: (created) => {
      setToken(created.token);
      queryClient.invalidateQueries({ queryKey: ['ingest-tokens'] });
      toast.success('Token created. Copy it now, it is shown once.');
    },
  });

  if (status?.last_span_at && status.last_span_at >= openedAt) {
    return <Navigate to="/traces" replace />;
  }

  const current = LANGUAGES.find((item) => item.value === language) ?? LANGUAGES[0];
  const env = `OTEL_SERVICE_NAME=api
OTEL_EXPORTER_OTLP_ENDPOINT=${window.location.origin}/v1/otlp
OTEL_EXPORTER_OTLP_PROTOCOL=http/protobuf
OTEL_EXPORTER_OTLP_HEADERS=authorization=Bearer ${token ?? '<token>'}`;

  return (
    <div className="flex min-h-full flex-col bg-surface-page">
      <PageBar backTo="/traces" icon="layers" title="Traces">
        <Button
          nativeButton={false}
          render={<a href="https://opentelemetry.io/docs/languages/" target="_blank" rel="noreferrer" />}
          variant="secondary"
          className="w-fit"
        >
          <Icon name="external-link" />
          OpenTelemetry Docs
        </Button>
      </PageBar>

      <div className="flex flex-1 flex-col justify-center gap-10 px-10 pt-3 pb-12 max-lg:px-4 xl:flex-row">
        <div className="flex w-full max-w-200 min-w-0 flex-col gap-6">
          <div className="flex flex-col gap-1">
            <h1 className="text-title-page font-semibold text-text-primary">Set Up Traces</h1>
            <p className="text-body text-text-secondary">
              Send traces and logs from your services with OpenTelemetry. Errors inside them show up in Issues, and
              each log line links to its trace.
            </p>
          </div>

          <Step title="1. Pick Your Language" description="Install the OpenTelemetry SDK and start your service with it.">
            <div role="tablist" aria-label="Language" className="flex flex-wrap gap-0.5">
              {LANGUAGES.map((item) => (
                <button
                  key={item.value}
                  type="button"
                  role="tab"
                  aria-selected={item.value === language}
                  onClick={() => setLanguage(item.value)}
                  className="h-7 rounded-sm px-2.5 text-body text-text-secondary outline-none hover:text-text-primary focus-visible:ring-2 focus-visible:ring-text-link/40 aria-selected:bg-surface-fill aria-selected:font-medium aria-selected:text-text-primary"
                >
                  {item.label}
                </button>
              ))}
            </div>
            <CodeBlock label={`${current.label} setup`} code={current.code} language="shellscript" />
          </Step>

          <Step
            title="2. Point the Exporter at Daroyan"
            description="Set these on the service. The same endpoint takes traces and logs. The token is shown once, so copy it before you leave."
          >
            <CodeBlock label="Exporter variables" code={env} language="shellscript" />
            {!token &&
              (isAdmin ? (
                <Button className="w-fit" disabled={create.isPending} onClick={() => create.mutate({ name: 'Traces' })}>
                  <Icon name="plus" />
                  Create Token
                </Button>
              ) : (
                <p className="text-body text-text-secondary">Ask an admin for a telemetry token.</p>
              ))}
          </Step>

          <Step title="3. Send a Test Span">
            <div className="flex items-center gap-3 rounded-2xl border border-border-ring bg-surface-default px-4 py-3.5 shadow-card">
              <Icon name="loader" className="animate-spin text-icon-secondary" />
              <div className="flex min-w-0 flex-1 flex-col gap-0.5">
                <p className="text-body font-medium text-text-primary">Waiting for the first trace</p>
                <p className="text-body text-text-secondary">
                  Start the service and make a request. This page moves on as soon as a span arrives.
                </p>
              </div>
              <Badge tone="neutral">No Spans Yet</Badge>
            </div>
          </Step>
        </div>

        <aside className="flex w-full shrink-0 flex-col gap-4 pt-2 xl:w-65">
          <RailSection title="What You Get">
            <RailRow label="Traces">Each request, span by span</RailRow>
            <RailRow label="Issues">Errors grouped by cause</RailRow>
            <RailRow label="Slow endpoints">p95 per route</RailRow>
            <RailRow label="Logs">Linked to their trace</RailRow>
          </RailSection>
          <RailSection title="Works With">
            <RailRow label="SDK">Any OpenTelemetry SDK</RailRow>
            <RailRow label="Protocol">OTLP over HTTP</RailRow>
            <RailRow label="Network">Outbound 443</RailRow>
            <RailRow label="Docs">
              <a
                href="https://opentelemetry.io"
                target="_blank"
                rel="noreferrer"
                className="outline-none hover:underline focus-visible:underline"
              >
                opentelemetry.io
              </a>
            </RailRow>
          </RailSection>
        </aside>
      </div>
    </div>
  );
}

type StepProps = {
  title: string;
  description?: string;
  children: ReactNode;
};

function Step(props: StepProps) {
  const { title, description, children } = props;

  return (
    <section className="flex flex-col gap-3">
      <div className="flex flex-col gap-0.5">
        <h2 className="text-title-section font-medium text-text-primary">{title}</h2>
        {description && <p className="text-body text-text-secondary">{description}</p>}
      </div>
      {children}
    </section>
  );
}
