import { createContext } from 'react';

// A sheet renders into the panel, so it covers the page and not the sidebar.
export const PanelContext = createContext<HTMLElement | null>(null);
