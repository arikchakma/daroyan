import type { CSSProperties } from 'react';
import { Toaster as Sonner } from 'sonner';

import { Icon } from '~/components/icons/icon';

export function Toaster() {
  return (
    <Sonner
      position="bottom-center"
      offset={{ bottom: 20 }}
      mobileOffset={{ bottom: 20 }}
      visibleToasts={3}
      gap={8}
      duration={4000}
      style={{ '--width': 'min(560px, calc(100vw - 32px))' } as CSSProperties}
      icons={{
        success: (
          <Icon name="circle-check-fill" className="text-toast-success-icon" />
        ),
        info: <Icon name="info-fill" className="text-toast-info-icon" />,
        warning: (
          <Icon
            name="triangle-alert-fill"
            className="text-toast-warning-icon"
          />
        ),
        error: <Icon name="circle-x-fill" className="text-text-on-dark" />,
        loading: <Icon name="loader" className="animate-spin" />,
      }}
      toastOptions={{
        unstyled: true,
        classNames: {
          toast:
            'group/toast right-0 left-0 mx-auto flex w-fit max-w-full items-start gap-2 rounded-[18px] bg-surface-code py-1.5 pr-1.5 pl-3.5 font-sans text-text-on-dark shadow-popover outline-none [--icon-glyph:var(--color-button-primary)] data-[expanded=false]:data-[front=false]:overflow-hidden data-[expanded=false]:data-[front=false]:*:opacity-0',
          error: 'bg-toast-error [--icon-glyph:var(--color-toast-error)]',
          icon: 'relative -ml-1 flex h-6 w-4 shrink-0 items-center justify-center',
          content: 'flex min-w-0 flex-col gap-0.5 py-0.5 pr-2',
          title: 'text-body font-medium',
          description:
            'text-small text-sidebar-text-muted group-data-[type=error]/toast:text-white/80',
          cancelButton:
            'flex h-6 shrink-0 cursor-pointer items-center rounded-full px-2 text-body font-medium text-sidebar-text-muted outline-none hover:text-text-on-dark focus-visible:ring-2 focus-visible:ring-white/40',
          actionButton:
            'flex h-6 shrink-0 cursor-pointer items-center rounded-full bg-sidebar-item-active px-2.5 text-body font-medium text-text-on-dark outline-none focus-visible:ring-2 focus-visible:ring-white/40 group-data-[type=error]/toast:bg-white/18',
        },
      }}
    />
  );
}
