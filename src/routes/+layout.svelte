<script lang="ts">
  import '../app.css';
  import { onMount } from 'svelte';
  import { branding, applyBrandingVars } from '$lib/stores/branding';
  import Toast from '$lib/components/Toast.svelte';

  onMount(async () => {
    await branding.load();
    applyBrandingVars($branding);
  });
</script>

<slot />
<Toast />
