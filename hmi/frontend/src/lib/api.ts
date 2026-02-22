export type UserRole = 'viewer' | 'operator' | 'engineer' | 'admin';

export type AuthMeResponse = {
  authenticated: boolean;
  username: string | null;
  role: UserRole | null;
  csrf_token: string | null;
  force_password_change: boolean;
};

export type LoginResponse = {
  username: string;
  role: UserRole;
  csrf_token: string;
  force_password_change: boolean;
};

let csrfToken: string | null = null;

export function setCsrfToken(token: string | null) {
  csrfToken = token;
}

function withDefaultHeaders(init?: RequestInit, includeCsrf = false): RequestInit {
  const headers = new Headers(init?.headers || {});
  if (includeCsrf && csrfToken) {
    headers.set('x-csrf-token', csrfToken);
  }
  return {
    ...init,
    headers,
    credentials: 'include',
  };
}

export async function apiFetch(input: RequestInfo | URL, init?: RequestInit, includeCsrf = false) {
  return fetch(input, withDefaultHeaders(init, includeCsrf));
}

export async function apiMe(): Promise<AuthMeResponse> {
  const response = await apiFetch('/api/auth/me');
  if (!response.ok) {
    return { authenticated: false, username: null, role: null, csrf_token: null, force_password_change: false };
  }
  const data = (await response.json()) as AuthMeResponse;
  setCsrfToken(data.csrf_token ?? null);
  return data;
}

export async function apiLogin(username: string, password: string): Promise<LoginResponse> {
  const response = await apiFetch('/api/auth/login', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ username, password }),
  });

  if (!response.ok) {
    const text = await response.text();
    throw new Error(text || 'Login failed');
  }

  const data = (await response.json()) as LoginResponse;
  setCsrfToken(data.csrf_token);
  return data;
}

export async function apiLogout() {
  const response = await apiFetch('/api/auth/logout', { method: 'POST' }, true);
  if (!response.ok && response.status !== 401) {
    const text = await response.text();
    throw new Error(text || 'Logout failed');
  }
  setCsrfToken(null);
}

export async function apiChangePassword(currentPassword: string, newPassword: string) {
  const response = await apiFetch('/api/auth/change-password', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ current_password: currentPassword, new_password: newPassword }),
  }, true);

  if (!response.ok) {
    const text = await response.text();
    throw new Error(text || 'Change password failed');
  }
}

export function hasRoleAtLeast(role: UserRole, required: UserRole): boolean {
  const rank: Record<UserRole, number> = {
    viewer: 1,
    operator: 2,
    engineer: 3,
    admin: 4,
  };
  return rank[role] >= rank[required];
}
