type ApiResult<D> = {
  data?: D;
  error?: unknown;
  response: Response;
};

export class ApiError extends Error {
  name: string;
  status: number;

  constructor(name: string, message: string, status: number) {
    super(message);
    this.name = name;
    this.status = status;
  }
}

function isErrorBody(value: unknown): value is { name: string; message: string } {
  return (
    typeof value === 'object' &&
    value !== null &&
    'name' in value &&
    'message' in value
  );
}

export async function request<D>(result: Promise<ApiResult<D>>): Promise<D> {
  const { data, error, response } = await result;

  if (response.status === 401) {
    document.cookie = 'daroyan_signed_in=; path=/; max-age=0';
    const next = window.location.pathname + window.location.search;
    window.location.href = `/login?next=${encodeURIComponent(next)}`;
    // The page unloads, so the caller must not continue.
    return new Promise<D>(() => {});
  }

  if (!response.ok) {
    if (isErrorBody(error)) {
      throw new ApiError(error.name, error.message, response.status);
    }
    throw new ApiError('unknown_error', 'An unexpected error occurred', response.status);
  }

  return data as D;
}
