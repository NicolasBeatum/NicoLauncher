import { writable } from 'svelte/store';

export type Toast = { id: number; kind: 'success' | 'error' | 'info'; message: string };

let nextId = 0;
const { subscribe, update } = writable<Toast[]>([]);

export const toasts = { subscribe };

export function addToast(kind: Toast['kind'], message: string, durationMs = 4000) {
  const id = nextId++;
  update(list => [...list, { id, kind, message }]);
  setTimeout(() => update(list => list.filter(t => t.id !== id)), durationMs);
}
