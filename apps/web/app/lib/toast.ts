import { toast as sonner } from 'sonner';
import type { ExternalToast } from 'sonner';

const SHORT_DURATION = 4000;
const LONG_DURATION = 8000;

type ToastMessage = Parameters<typeof sonner>[0];

export const toast = Object.assign(
  (message: ToastMessage, data?: ExternalToast) =>
    sonner(message, { duration: SHORT_DURATION, ...data }),
  {
    success: (message: ToastMessage, data?: ExternalToast) =>
      sonner.success(message, { duration: SHORT_DURATION, ...data }),
    info: (message: ToastMessage, data?: ExternalToast) =>
      sonner.info(message, { duration: SHORT_DURATION, ...data }),
    message: (message: ToastMessage, data?: ExternalToast) =>
      sonner.message(message, { duration: SHORT_DURATION, ...data }),
    promise: sonner.promise,
    custom: sonner.custom,
    dismiss: sonner.dismiss,
    error: (message: ToastMessage, data?: ExternalToast) =>
      sonner.error(message, { duration: LONG_DURATION, ...data }),
    warning: (message: ToastMessage, data?: ExternalToast) =>
      sonner.warning(message, { duration: LONG_DURATION, ...data }),
    loading: (message: ToastMessage, data?: ExternalToast) =>
      sonner.loading(message, { duration: Infinity, ...data }),
  }
);

type GetErrorMessageOptions = {
  error: unknown;
  fallback: string;
};

export function getErrorMessage(options: GetErrorMessageOptions): string {
  const { error, fallback } = options;

  return error instanceof Error && error.message ? error.message : fallback;
}

type ToastErrorOptions = {
  error: unknown;
  fallback: string;
};

export function getToastError(options: ToastErrorOptions): void {
  const { error, fallback } = options;

  toast.error(getErrorMessage({ error, fallback }));
}
