import { Icon } from '~/components/icons/icon';
import { Popover } from '~/components/ui/popover';

const EXAMPLES = [
  { code: 'timeout refused', meaning: 'Message contains timeout refused' },
  { code: '-timeout', meaning: 'Message does not contain timeout' },
  { code: '"user:42 failed"', meaning: 'Quotes keep : and - as plain text' },
  { code: '/time(d)?out/', meaning: 'Message matches a regex' },
  { code: 'service:api', meaning: 'Field is api' },
  { code: '-service:api', meaning: 'Field is not api' },
  { code: 'service:[api,web]', meaning: 'Field is either value' },
  { code: 'path:*/v1/*', meaning: 'Field contains, * matches anything' },
  { code: 'status:>499', meaning: 'Compare numbers with > or <' },
  { code: 'user.id:*', meaning: 'Field exists' },
  { code: '-user.id:*', meaning: 'Field is missing' },
  { code: '*:usr_42', meaning: 'Any field contains usr_42' },
];

export function SearchSyntaxPopover() {
  return (
    <Popover.Root>
      <Popover.Trigger
        aria-label="Search Syntax"
        title="Search syntax"
        className="flex size-7 shrink-0 items-center justify-center rounded-sm text-text-secondary outline-none hover:bg-surface-fill focus-visible:ring-2 focus-visible:ring-text-link/40 data-popup-open:bg-surface-fill"
      >
        <Icon name="circle-help" />
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Positioner align="end" sideOffset={6}>
          <Popover.Popup className="w-120 max-w-[calc(100vw-2rem)] px-4 py-3.5">
            <Popover.Title>Search Syntax</Popover.Title>
            <Popover.Description className="text-caption text-text-tertiary">
              Type these, or pick from the menu. Every pill has to match.
            </Popover.Description>
            <dl className="mt-2.5 flex flex-col">
              {EXAMPLES.map((example) => (
                <div
                  key={example.code}
                  className="flex items-center gap-3 border-b border-border-row py-1.5 last:border-b-0"
                >
                  <dt className="w-42 shrink-0">
                    <code className="rounded-xs bg-surface-fill px-1.5 py-0.5 font-mono text-code">
                      {example.code}
                    </code>
                  </dt>
                  <dd className="min-w-0 flex-1 text-body text-text-secondary">{example.meaning}</dd>
                </div>
              ))}
            </dl>
            <p className="mt-2.5 text-caption text-text-tertiary">
              Search ignores case. “is not” and “does not contain” also match lines that do not have
              the field. Regex uses RE2.
            </p>
          </Popover.Popup>
        </Popover.Positioner>
      </Popover.Portal>
    </Popover.Root>
  );
}
