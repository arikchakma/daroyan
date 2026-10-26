import { QueryClientProvider } from '@tanstack/react-query';
import { Links, Meta, Outlet, Scripts, ScrollRestoration } from 'react-router';

import { Toaster } from '~/components/ui/toaster';
import { queryClient } from '~/lib/query-client';
import { getThemeRootProps, THEME_SCRIPT } from '~/utils/theme';

import type { Route } from './+types/root';

import './app.css';
import { IconSprite } from './components/icons/icon-sprite';
import { NavigationProgress } from './components/navigation/navigation-progress';

export const links: Route.LinksFunction = () => {
  return [
    { rel: 'icon', href: '/favicon.ico', sizes: '32x32' },
    { rel: 'icon', href: '/icon.svg', type: 'image/svg+xml' },
    { rel: 'apple-touch-icon', href: '/apple-touch-icon.png' },
    { rel: 'manifest', href: '/manifest.webmanifest' },
    { rel: 'preconnect', href: 'https://fonts.googleapis.com' },
    {
      rel: 'preconnect',
      href: 'https://fonts.gstatic.com',
      crossOrigin: 'anonymous',
    },
    {
      rel: 'stylesheet',
      href: 'https://fonts.googleapis.com/css2?family=Inter:wght@400..700&family=Geist+Mono:wght@400..500&display=swap',
    },
  ];
};

export const meta: Route.MetaFunction = () => [
  { title: 'Daroyan' },
  {
    name: 'description',
    content: 'Watch your servers, processes and ports in one place.',
  },
];

export function Layout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" {...getThemeRootProps()}>
      <head>
        <meta charSet="utf-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1" />
        <meta name="theme-color" content="#4f63ff" />
        <script dangerouslySetInnerHTML={{ __html: THEME_SCRIPT }} />
        <Meta />
        <Links />
      </head>
      <body className="bg-sidebar-bg text-gray-100 antialiased selection:bg-zinc-200 selection:text-zinc-900">
        <IconSprite />
        <QueryClientProvider client={queryClient}>
          {children}
          <ScrollRestoration />
          <Scripts />
          <Toaster />
          <NavigationProgress />
        </QueryClientProvider>
      </body>
    </html>
  );
}

export default function App() {
  return <Outlet />;
}

export function ErrorBoundary(props: Route.ErrorBoundaryProps) {
  const { error } = props;

  return (
    <div className="flex h-dvh bg-sidebar-bg p-1">
      <main className="flex flex-1 flex-col items-center justify-center gap-2 rounded-xl bg-surface-default p-6 text-center outline -outline-offset-1 outline-border-panel">
        <p className="text-title-section font-medium text-text-primary">Something Went Wrong</p>
        <p className="max-w-120 text-body text-text-secondary">
          {error instanceof Error ? error.message : 'The page could not load.'}
        </p>
        <button
          type="button"
          onClick={() => window.location.reload()}
          className="mt-2 h-8 rounded-full bg-button-primary px-3 text-body font-medium text-text-on-primary shadow-button-primary"
        >
          Reload
        </button>
      </main>
    </div>
  );
}

export function HydrateFallback() {
  return (
    <div className="flex h-dvh bg-sidebar-bg p-1">
      <div className="flex-1 rounded-xl bg-surface-page outline -outline-offset-1 outline-border-panel" />
    </div>
  );
}
