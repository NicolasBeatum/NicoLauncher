<script lang="ts">
  import { branding } from '$lib/stores/branding';
  import { login, session } from '$lib/stores/auth';
  import { screen } from '$lib/stores/screen';
  import { addToast } from '$lib/stores/toast';
  import { api } from '$lib/tauri';
  import { fade, fly } from 'svelte/transition';

  let loading = false;
  let offlineUsername = '';
  let showOffline = false;

  async function handleLogin() {
    loading = true;
    try {
      await login();
      screen.set('home');
    } catch (e) {
      addToast('error', String(e));
    } finally {
      loading = false;
    }
  }

  async function handleOfflineLogin() {
    loading = true;
    try {
      const s = await api.authLoginOffline(offlineUsername || 'Jugador');
      session.set(s);
      screen.set('home');
    } catch (e) {
      addToast('error', String(e));
    } finally {
      loading = false;
    }
  }
</script>

<div
  in:fade={{ duration: 300 }}
  out:fade={{ duration: 200 }}
  class="fixed inset-0 flex items-center justify-center"
  style="background: var(--color-secondary)"
>
  <!-- Background gradient -->
  <div class="absolute inset-0 opacity-20"
       style="background: radial-gradient(ellipse at 50% 0%, var(--color-primary) 0%, transparent 70%)">
  </div>

  <div in:fly={{ y: 20, duration: 400, delay: 100 }}
       class="relative z-10 flex flex-col items-center gap-8 p-10 rounded-2xl w-full max-w-sm"
       style="background: rgba(255,255,255,0.05); backdrop-filter: blur(16px)">

    <!-- Logo -->
    <div class="w-20 h-20 rounded-2xl flex items-center justify-center shadow-xl"
         style="background: var(--color-primary)">
      <span class="text-3xl font-bold text-white">
        {$branding.displayName.charAt(0)}
      </span>
    </div>

    <div class="text-center">
      <h1 class="text-2xl font-bold text-white font-heading">{$branding.displayName}</h1>
      <p class="text-white/50 text-sm mt-1">Inicia sesión para jugar</p>
    </div>

    <button
      on:click={handleLogin}
      disabled={loading}
      class="w-full py-3 px-6 rounded-xl font-semibold text-white transition-all duration-200
             disabled:opacity-50 disabled:cursor-not-allowed
             hover:brightness-110 active:scale-95"
      style="background: var(--color-primary)"
    >
      {#if loading}
        <span class="flex items-center justify-center gap-2">
          <span class="w-4 h-4 border-2 border-white/30 border-t-white rounded-full animate-spin"></span>
          Abriendo navegador…
        </span>
      {:else}
        Iniciar sesión con Microsoft
      {/if}
    </button>

    <p class="text-white/30 text-xs text-center">
      Se abrirá una ventana del navegador para autenticarte con tu cuenta de Microsoft.
    </p>

    <!-- Offline mode -->
    <div class="w-full border-t border-white/10 pt-4">
      {#if !showOffline}
        <button on:click={() => showOffline = true}
                class="w-full text-white/30 hover:text-white/60 text-xs transition-colors">
          Modo offline (desarrollo)
        </button>
      {:else}
        <div class="flex flex-col gap-2" in:fly={{ y: 6, duration: 200 }}>
          <input
            type="text"
            bind:value={offlineUsername}
            placeholder="Nombre de jugador"
            maxlength="16"
            class="w-full bg-white/10 text-white text-sm px-3 py-2 rounded-lg
                   placeholder:text-white/30 border border-white/10 focus:outline-none
                   focus:border-white/30"
          />
          <button on:click={handleOfflineLogin} disabled={loading}
                  class="w-full py-2 px-4 rounded-lg text-sm font-medium text-white/70
                         border border-white/20 hover:border-white/40 hover:text-white
                         disabled:opacity-50 transition-all">
            Entrar offline
          </button>
        </div>
      {/if}
    </div>

  </div>
</div>
