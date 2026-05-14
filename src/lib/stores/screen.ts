import { writable } from 'svelte/store';

export type Screen = 'splash' | 'login' | 'home' | 'optional-mods' | 'settings';

export const screen = writable<Screen>('splash');
