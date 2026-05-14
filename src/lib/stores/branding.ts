import { writable } from 'svelte/store';
import { api, type BrandingDto } from '$lib/tauri';

const defaults: BrandingDto = {
  internalId: 'mc-launcher-template', displayName: 'MC Launcher', windowTitle: 'MC Launcher',
  primaryColor: '#7c3aed', secondaryColor: '#1e293b', accentColor: '#f59e0b',
  headingFont: 'Inter', bodyFont: 'Inter', discord: '', website: '',
  serverName: 'Mi Servidor', serverAddress: '',
};

function createBrandingStore() {
  const { subscribe, set, update } = writable<BrandingDto>(defaults);
  return {
    subscribe,
    load: async () => {
      try { set(await api.getBranding()); } catch { /* use defaults */ }
    },
  };
}

export const branding = createBrandingStore();

export function applyBrandingVars(b: BrandingDto) {
  const root = document.documentElement;
  root.style.setProperty('--color-primary',   b.primaryColor);
  root.style.setProperty('--color-secondary', b.secondaryColor);
  root.style.setProperty('--color-accent',    b.accentColor);
  root.style.setProperty('--font-heading',    b.headingFont);
  root.style.setProperty('--font-body',       b.bodyFont);
}
