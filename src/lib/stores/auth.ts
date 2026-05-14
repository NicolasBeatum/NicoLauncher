import { writable } from 'svelte/store';
import { api, type AuthSessionDto } from '$lib/tauri';

export const session = writable<AuthSessionDto | null>(null);

export async function tryResumeSession(): Promise<boolean> {
  try {
    const s = await api.authRefresh();
    session.set(s);
    return true;
  } catch {
    session.set(null);
    return false;
  }
}

export async function login(): Promise<void> {
  const s = await api.authLogin();
  session.set(s);
}

export async function logout(): Promise<void> {
  await api.authLogout();
  session.set(null);
}
