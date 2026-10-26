import { useState } from 'react';
import { Link, NavLink } from 'react-router';

import { Icon } from '~/components/icons/icon';
import { SETTINGS_ITEMS } from '~/utils/navigation';

export function SettingsSidebar() {
  const [search, setSearch] = useState('');

  const items = SETTINGS_ITEMS.filter((item) =>
    item.label.toLowerCase().includes(search.trim().toLowerCase())
  );

  return (
    <nav
      aria-label="Settings"
      className="flex h-full w-60 shrink-0 flex-col overflow-y-auto border-l-[0.5px] border-sidebar-divider bg-sidebar-bg px-2.5 pt-3 whitespace-nowrap scheme-dark"
    >
      <Link
        to="/servers"
        className="flex h-8 shrink-0 items-center gap-2 rounded-lg pl-2 text-body font-medium text-white outline-none hover:bg-white/5 focus-visible:ring-2 focus-visible:ring-white/30"
      >
        <Icon name="chevron-left" className="size-3.5 text-sidebar-text-muted" />
        Settings
      </Link>

      <label className="mt-2.5 flex h-8 shrink-0 items-center gap-2 rounded-lg bg-sidebar-search px-2.5 text-sidebar-label focus-within:ring-2 focus-within:ring-white/30">
        <Icon name="search" />
        <input
          type="search"
          aria-label="Search Settings"
          placeholder="Search settings"
          value={search}
          onChange={(event) => setSearch(event.target.value)}
          className="min-w-0 flex-1 bg-transparent text-body text-white outline-none placeholder:text-sidebar-label"
        />
      </label>

      <div className="mt-3.5 flex shrink-0 flex-col">
        {items.map((item) => (
          <NavLink
            key={item.path}
            to={item.path}
            prefetch="intent"
            className="group flex h-7.5 shrink-0 items-center gap-2.5 rounded-lg pr-2 pl-2.5 text-body font-medium text-sidebar-text outline-none hover:bg-white/5 focus-visible:ring-2 focus-visible:ring-white/30 aria-[current=page]:bg-sidebar-item-active aria-[current=page]:text-white"
          >
            <Icon name={item.icon} className="text-sidebar-text-muted group-aria-[current=page]:text-white" />
            <span className="truncate">{item.label}</span>
          </NavLink>
        ))}
        {items.length === 0 && <p className="px-2.5 py-1.5 text-body text-sidebar-label">No settings match.</p>}
      </div>
    </nav>
  );
}
