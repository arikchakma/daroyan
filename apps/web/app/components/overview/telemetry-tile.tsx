import type { Schemas } from '@daroyan/client';

import { Icon } from '~/components/icons/icon';
import { Tile } from '~/components/overview/tile';
import { TileRow } from '~/components/overview/tile-row';
import { TrendBars } from '~/components/overview/trend-bars';
import { Badge } from '~/components/ui/badge';
import { DAY_MS, HOUR_MS, pluralize } from '~/utils/format';

type TelemetryTileProps = {
  issues: Schemas['Issue'][];
  errors: Schemas['TrendPoint'][];
  isSetUp: boolean;
};

export function TelemetryTile(props: TelemetryTileProps) {
  const { issues, errors, isSetUp } = props;

  const newIssues = issues.filter((issue) => issue.first_seen_at / 1_000 > Date.now() - DAY_MS);
  const hourErrors = errors.find((point) => point.at === Math.floor(Date.now() / HOUR_MS) * HOUR_MS)?.value ?? 0;

  if (!isSetUp) {
    return (
      <Tile
        title="Telemetry"
        href="/traces/setup"
        linkLabel="Set Up"
        value="No traces yet"
        caption="send spans from any OpenTelemetry SDK"
        chart={<TrendBars points={[]} label={String} minScale={1} fillClassName="bg-brand" />}
        healthy=""
      >
        <TileRow
          href="/traces/setup"
          badge={
            <Badge>
              <Icon name="terminal" />
              Setup
            </Badge>
          }
          label="Set up traces to see errors and slow requests"
          meta=""
        />
      </Tile>
    );
  }

  return (
    <Tile
      title="Telemetry"
      href="/issues"
      linkLabel="View all issues"
      value={newIssues.length === 0 ? 'No new issues' : pluralize(newIssues.length, 'new issue')}
      caption={`${pluralize(hourErrors, 'error')} this hour`}
      chart={
        <TrendBars
          points={errors}
          label={(value) => pluralize(value, 'error')}
          minScale={4}
          fillClassName="bg-brand"
        />
      }
      healthy="No new errors today."
    >
      {newIssues.slice(0, 1).map((issue) => (
        <TileRow
          key={issue.fingerprint}
          more={newIssues.length - 1}
          href={`/issues/${issue.fingerprint}`}
          badge={
            <Badge>
              <span className="font-mono">{issue.service}</span>
            </Badge>
          }
          label={issue.kind}
          isMono
          meta={pluralize(issue.count, 'event')}
        />
      ))}
    </Tile>
  );
}
