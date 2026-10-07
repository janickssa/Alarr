// Typed API client generated from openapi/openapi.json (see `npm run gen:api`).
import createClient, { type Middleware } from 'openapi-fetch';
import type { components, paths } from './schema';
import { connection } from '#lib/connection.svelte.js';

export type Schemas = components['schemas'];
export type ServiceView = Schemas['ServiceView'];
export type ServiceSnapshot = Schemas['ServiceSnapshot'];
export type Health = Schemas['Health'];
export type Guest = Schemas['Guest'];
export type GuestType = Schemas['GuestType'];
export type GuestStatus = Schemas['GuestStatus'];
export type PowerAction = Schemas['PowerAction'];
export type Node = Schemas['Node'];
export type StoragePool = Schemas['StoragePool'];
export type RrdPoint = Schemas['RrdPoint'];
export type RrdRange = 'hour' | 'day' | 'week';
export type Task = Schemas['Task'];
export type User = Schemas['User'];
export type Role = Schemas['Role'];
export type Device = Schemas['Device'];
export type ActivityEntry = Schemas['ActivityEntry'];
export type CatalogueEntry = Schemas['CatalogueEntry'];
export type ConnectionTest = Schemas['ConnectionTest'];
export type Category = Schemas['Category'];
export type ServiceKind = Schemas['ServiceKind'];
export type Overview = Schemas['Overview'];
export type HealthReport = Schemas['HealthReport'];
export type HealthEntry = Schemas['HealthEntry'];
export type TimelineBucket = Schemas['TimelineBucket'];
export type Me = Schemas['Me'];
export type ProxmoxData = Schemas['ProxmoxData'];

/** An error response from the API, with the server's message. */
export class ApiError extends Error {
	constructor(
		public status: number,
		public code: string,
		message: string
	) {
		super(message);
	}
}

const tracking: Middleware = {
	onResponse({ response, request }) {
		connection.markReachable();
		if (response.status === 401 && !new URL(request.url).pathname.includes('/auth/')) {
			connection.markSignedOut();
		}
		return undefined;
	},
	onError({ error }) {
		connection.markUnreachable();
		return error instanceof Error ? error : new Error(String(error));
	}
};

export const api = createClient<paths>({ baseUrl: '/api/v1', credentials: 'same-origin' });
api.use(tracking);

type Result<T> = { data?: T; error?: unknown; response: Response };

/** Return the data of a call or throw an `ApiError` with the server's message. */
export function unwrap<T>(result: Result<T>): T {
	if (result.error !== undefined || !result.response.ok) {
		const body = (result.error ?? {}) as { error?: string; message?: string };
		throw new ApiError(
			result.response.status,
			body.error ?? 'error',
			body.message ?? `HTTP ${result.response.status}`
		);
	}
	return result.data as T;
}

/** A message for showing an unknown error to the user. */
export function errorMessage(e: unknown): string {
	if (e instanceof ApiError) return e.message;
	if (e instanceof TypeError) return "Can't reach the backend";
	return e instanceof Error ? e.message : String(e);
}
