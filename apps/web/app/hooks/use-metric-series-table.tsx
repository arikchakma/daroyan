import { createColumnHelper } from '@tanstack/react-table';
import { useMemo } from 'react';

import { Dash } from '~/components/ui/dash';
import { useDataTable } from '~/hooks/use-data-table';
import type { DataTableFeatures } from '~/lib/table';
import { formatMetricValue } from '~/utils/metrics';
import type { MetricRead } from '~/utils/metrics';

export type MetricSeriesRow = {
  id: string;
  color: string;
  values: (number | undefined)[];
};

const helper = createColumnHelper<DataTableFeatures, MetricSeriesRow>();

type UseMetricSeriesTableOptions = {
  rows: MetricSeriesRow[];
  groupLabel: string;
  reads: MetricRead[];
  unit: string;
};

export function useMetricSeriesTable(options: UseMetricSeriesTableOptions) {
  const { rows, groupLabel, reads, unit } = options;

  const columns = useMemo(
    () =>
      helper.columns([
        helper.accessor('id', {
          header: groupLabel,
          enableSorting: false,
          cell: ({ row }) => (
            <span className="flex min-w-0 items-center gap-2.5">
              <span className="size-1.5 shrink-0 rounded-full" style={{ backgroundColor: row.original.color }} />
              <span className="truncate font-medium">{row.original.id}</span>
            </span>
          ),
        }),
        ...reads.map((read, index) =>
          helper.display({
            id: `${read.aggregation}-${index}`,
            header: read.label,
            meta: { width: index === 0 ? 120 : 84, align: 'right', hideBelow: index > 2 ? 'md' : undefined },
            cell: ({ row }) => {
              const value = row.original.values[index];
              return value === undefined ? <Dash /> : formatMetricValue(value, unit, read.aggregation);
            },
          })
        ),
      ]),
    [groupLabel, reads, unit]
  );

  return useDataTable({ columns, data: rows });
}
