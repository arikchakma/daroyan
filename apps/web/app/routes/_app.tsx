import { useHotkey } from '@tanstack/react-hotkeys';
import { useState } from 'react';
import { Outlet, redirect, useLocation } from 'react-router';

import { AppSidebar } from '~/components/navigation/app-sidebar';
import { CommandMenu } from '~/components/navigation/command-menu';
import { PanelContext } from '~/components/navigation/panel-context';
import { SettingsSidebar } from '~/components/navigation/settings-sidebar';
import { Sheet } from '~/components/ui/sheet';
import { getCommandGroups } from '~/hooks/use-command-actions';
import type { CommandGroup } from '~/hooks/use-command-actions';
import { isLoggedIn } from '~/lib/auth';

export function clientLoader({ request }: { request: Request }) {
  if (!isLoggedIn()) {
    const url = new URL(request.url);
    throw redirect(`/login?next=${encodeURIComponent(url.pathname + url.search)}`);
  }
  return null;
}

export default function AppLayout() {
  const { pathname } = useLocation();
  const isSettings = pathname === '/settings' || pathname.startsWith('/settings/');
  const [panel, setPanel] = useState<HTMLDivElement | null>(null);
  const [isCommandOpen, setIsCommandOpen] = useState(false);
  const [pageGroups, setPageGroups] = useState<CommandGroup[]>([]);

  const openCommandMenu = () => {
    setPageGroups(getCommandGroups());
    setIsCommandOpen(true);
  };

  useHotkey('Mod+K', () => (isCommandOpen ? setIsCommandOpen(false) : openCommandMenu()));

  return (
    <Sheet.Provider>
      <div className="flex h-dvh bg-sidebar-bg">
        <AppSidebar isCompact={isSettings} onSearch={openCommandMenu} />
        {isSettings && <SettingsSidebar />}
        <div ref={setPanel} className="relative flex min-w-0 flex-1 overflow-clip py-1 pr-1">
          <Sheet.Indent
            render={<main />}
            className="min-w-0 flex-1 origin-top overflow-hidden rounded-xl text-text-primary outline -outline-offset-1 outline-border-panel transition-transform duration-150 ease-out data-active:-translate-y-[calc(2px*(1-var(--drawer-swipe-progress)))] data-active:scale-[calc(0.96+0.04*var(--drawer-swipe-progress))]"
          >
            <div className="h-full scrollbar-gutter-stable overflow-y-auto bg-surface-page has-[>.bg-surface-default]:bg-surface-default">
              <PanelContext value={panel}>
                <Outlet />
              </PanelContext>
            </div>
          </Sheet.Indent>
        </div>
      </div>
      <CommandMenu open={isCommandOpen} pageGroups={pageGroups} onOpenChange={setIsCommandOpen} />
    </Sheet.Provider>
  );
}
