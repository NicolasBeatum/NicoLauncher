<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { toasts, addToast } from '$lib/stores/toast';
  import { events } from '$lib/tauri';
  import { fly } from 'svelte/transition';

  let unlisten: (() => void) | null = null;

  onMount(() => {
    events.onToast(t => addToast(t.kind, t.message)).then(u => { unlisten = u; });
  });

  onDestroy(() => {
    unlisten?.();
  });

  const icons = { success: '✓', error: '✗', info: 'ℹ' };
  const colors = {
    success: 'bg-green-600',
    error:   'bg-red-600',
    info:    'bg-blue-600',
  };
</script>

<div class="fixed bottom-4 right-4 z-50 flex flex-col gap-2 pointer-events-none">
  {#each $toasts as t (t.id)}
    <div
      transition:fly={{ y: 20, duration: 250 }}
      class="flex items-center gap-3 px-4 py-3 rounded-lg shadow-xl text-white text-sm font-medium {colors[t.kind]}"
    >
      <span>{icons[t.kind]}</span>
      <span>{t.message}</span>
    </div>
  {/each}
</div>
