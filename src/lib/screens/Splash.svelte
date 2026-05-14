<script lang="ts">
  import { branding } from '$lib/stores/branding';
  import { fade } from 'svelte/transition';
</script>

<div
  in:fade={{ duration: 300 }}
  out:fade={{ duration: 200 }}
  class="fixed inset-0 flex flex-col items-center justify-center"
  style="background: var(--color-secondary)"
>
  <!-- Logo placeholder — replace with assets/logo.png -->
  <div class="w-24 h-24 rounded-2xl flex items-center justify-center mb-8 shadow-2xl"
       style="background: var(--color-primary)">
    <span class="text-4xl font-bold text-white">
      {$branding.displayName.charAt(0)}
    </span>
  </div>

  <h1 class="text-3xl font-bold text-white mb-2 font-heading">
    {$branding.displayName}
  </h1>

  <p class="text-white/50 text-sm mb-12">Inicializando…</p>

  <!-- Spinner -->
  <div class="w-8 h-8 border-2 border-white/20 border-t-white/80 rounded-full animate-spin"></div>
</div>
