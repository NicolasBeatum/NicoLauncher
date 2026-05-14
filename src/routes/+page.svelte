<script lang="ts">
  import { onMount } from 'svelte';
  import { screen } from '$lib/stores/screen';
  import { session, tryResumeSession } from '$lib/stores/auth';

  import Splash    from '$lib/screens/Splash.svelte';
  import Login     from '$lib/screens/Login.svelte';
  import Home      from '$lib/screens/Home.svelte';
  import OptMods   from '$lib/screens/OptionalMods.svelte';
  import Settings  from '$lib/screens/Settings.svelte';

  onMount(async () => {
    // After splash: try to resume saved session
    await new Promise(r => setTimeout(r, 1800));
    const ok = await tryResumeSession();
    screen.set(ok ? 'home' : 'login');
  });
</script>

{#if $screen === 'splash'}
  <Splash />
{:else if $screen === 'login'}
  <Login />
{:else}
  <!--
    Home se mantiene montado mientras el usuario esté logueado,
    así preserva su estado (manifest, syncPlan, logs, playState…)
    aunque el usuario visite Ajustes o Mods opcionales.
  -->
  <div style:display={$screen === 'home' ? 'contents' : 'none'}>
    <Home />
  </div>

  {#if $screen === 'optional-mods'}
    <OptMods />
  {:else if $screen === 'settings'}
    <Settings />
  {/if}
{/if}
