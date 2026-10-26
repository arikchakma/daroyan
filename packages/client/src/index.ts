import createClient from 'openapi-fetch';
import type { ClientOptions } from 'openapi-fetch';

import type { components, operations, paths } from './schema';

export type { components, operations, paths };
export type Schemas = components['schemas'];

export function createApiClient(options: ClientOptions) {
  return createClient<paths>(options);
}

export type ApiClient = ReturnType<typeof createApiClient>;
