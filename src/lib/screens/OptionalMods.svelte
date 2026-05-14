<script lang="ts">
  import { onMount } from 'svelte';
  import { fade, slide } from 'svelte/transition';
  import { screen } from '$lib/stores/screen';
  import { addToast } from '$lib/stores/toast';
  import { api, type OptionalModDto, type UserModDto } from '$lib/tauri';

  // ── State ──────────────────────────────────────────────────────────────────
  let serverMods: OptionalModDto[] = [];
  let userMods: UserModDto[] = [];
  let loadingServer = true;
  let loadingUser = true;
  let rebuilding = false;

  // Which tab is active: 'server' | 'user'
  let activeTab: 'server' | 'user' = 'server';

  // Per-mod loading state (while toggling)
  let toggling = new Set<string>();

  // ── Dependency/conflict resolution ─────────────────────────────────────────
  interface Resolution {
    modId:       string;
    enabling:    boolean;
    /** IDs de dependencias no activas que habría que activar también */
    depsToEnable:  string[];
    /** IDs de mods activos que entran en conflicto (si se activa este) */
    conflicts:    string[];
    /** IDs de mods activos que dependen de este (si se desactiva este) */
    dependents:   string[];
  }

  let pendingResolution: Resolution | null = null;

  /** Nombre legible de un mod por su ID */
  function modName(id: string): string {
    return serverMods.find(m => m.id === id)?.name ?? id;
  }

  /** Calcula si se necesita resolución antes de cambiar el estado de un mod.
   *  Devuelve null si no hay nada que resolver (toggle inmediato). */
  function computeResolution(mod: OptionalModDto, enabling: boolean): Resolution | null {
    if (enabling) {
      const depsToEnable = mod.dependsOn
        .map(id => serverMods.find(m => m.id === id))
        .filter((m): m is OptionalModDto => !!m && !m.enabled)
        .map(m => m.id);

      const conflicts = mod.conflictsWith
        .map(id => serverMods.find(m => m.id === id))
        .filter((m): m is OptionalModDto => !!m && m.enabled)
        .map(m => m.id);

      if (depsToEnable.length > 0 || conflicts.length > 0) {
        return { modId: mod.id, enabling: true, depsToEnable, conflicts, dependents: [] };
      }
    } else {
      const dependents = serverMods
        .filter(m => m.enabled && m.id !== mod.id && m.dependsOn.includes(mod.id))
        .map(m => m.id);

      if (dependents.length > 0) {
        return { modId: mod.id, enabling: false, depsToEnable: [], conflicts: [], dependents };
      }
    }
    return null;
  }

  // ── Carga ──────────────────────────────────────────────────────────────────
  onMount(async () => {
    await Promise.all([loadServerMods(), loadUserMods()]);
  });

  async function loadServerMods() {
    loadingServer = true;
    try {
      serverMods = await api.manifestOptionalModsList();
    } catch (e) {
      const msg = String(e);
      if (!msg.includes('No manifest loaded')) addToast('error', msg);
      serverMods = [];
    } finally {
      loadingServer = false;
    }
  }

  async function loadUserMods() {
    loadingUser = true;
    try {
      userMods = await api.userModsList();
    } catch (e) {
      addToast('error', String(e));
    } finally {
      loadingUser = false;
    }
  }

  // ── Server mod toggle ──────────────────────────────────────────────────────

  /** Llamado al hacer clic en el toggle — puede abrir el panel de resolución. */
  function handleToggleClick(mod: OptionalModDto) {
    if (toggling.has(mod.id)) return;

    // Si ya hay una resolución pendiente para este mod, cancelarla
    if (pendingResolution?.modId === mod.id) {
      pendingResolution = null;
      return;
    }

    const enabling = !mod.enabled;
    const resolution = computeResolution(mod, enabling);

    if (resolution) {
      pendingResolution = resolution;
    } else {
      doToggleServer(mod, enabling);
    }
  }

  /** Ejecuta el toggle efectivo (sin chequear resolución). */
  async function doToggleServer(mod: OptionalModDto, enable: boolean) {
    if (toggling.has(mod.id)) return;
    toggling = new Set([...toggling, mod.id]);
    try {
      const linkedNow = await api.manifestOptionalModSetEnabled(mod.id, enable);
      serverMods = await api.manifestOptionalModsList();
      if (enable && !linkedNow) {
        addToast('info', `${mod.name} habilitado — se descargará en el próximo sync`);
      }
    } catch (e) {
      addToast('error', String(e));
    } finally {
      toggling.delete(mod.id);
      toggling = new Set(toggling);
    }
  }

  /**
   * Aplica la resolución.
   * @param includeRelated  true → también activa deps / desactiva conflictos y dependents
   *                        false → solo activa/desactiva el mod original
   */
  async function applyResolution(includeRelated: boolean) {
    if (!pendingResolution) return;
    const res = pendingResolution;
    pendingResolution = null;

    const mod = serverMods.find(m => m.id === res.modId);
    if (!mod) return;

    if (includeRelated && res.enabling) {
      // 1. Activar dependencias faltantes primero
      for (const depId of res.depsToEnable) {
        const dep = serverMods.find(m => m.id === depId);
        if (dep) await doToggleServer(dep, true);
      }
      // 2. Desactivar conflictos
      for (const conflictId of res.conflicts) {
        const conflict = serverMods.find(m => m.id === conflictId);
        if (conflict) await doToggleServer(conflict, false);
      }
    } else if (includeRelated && !res.enabling) {
      // Desactivar dependientes
      for (const depId of res.dependents) {
        const dep = serverMods.find(m => m.id === depId);
        if (dep) await doToggleServer(dep, false);
      }
    }

    // Actualizar referencia (el estado pudo cambiar durante los toggles anteriores)
    const fresh = serverMods.find(m => m.id === res.modId);
    if (fresh) await doToggleServer(fresh, res.enabling);
  }

  // ── Rebuild ────────────────────────────────────────────────────────────────
  async function rebuild() {
    rebuilding = true;
    try {
      const missing = await api.syncRebuildOptional();
      await loadServerMods();
      if (missing.length === 0) {
        addToast('success', 'Mods opcionales reconstruidos correctamente');
      } else {
        addToast('info', `${missing.length} mod${missing.length !== 1 ? 's' : ''} no estaban en caché: ${missing.join(', ')} — ejecuta sync para descargarlos`);
      }
    } catch (e) {
      addToast('error', String(e));
    } finally {
      rebuilding = false;
    }
  }

  // ── User mod toggle ────────────────────────────────────────────────────────
  async function toggleUser(mod: UserModDto) {
    if (toggling.has(mod.filename)) return;
    toggling = new Set([...toggling, mod.filename]);
    try {
      await api.userModSetEnabled(mod.filename, !mod.enabled);
      userMods = userMods.map(m =>
        m.filename === mod.filename ? { ...m, enabled: !m.enabled } : m
      );
    } catch (e) {
      addToast('error', String(e));
      await loadUserMods();
    } finally {
      toggling.delete(mod.filename);
      toggling = new Set(toggling);
    }
  }

  async function openUserFolder() {
    try { await api.userModsOpenFolder(); }
    catch (e) { addToast('error', String(e)); }
  }

  function formatSize(b: number) {
    if (b < 1024) return `${b} B`;
    if (b < 1024 * 1024) return `${(b / 1024).toFixed(0)} KB`;
    return `${(b / 1024 / 1024).toFixed(1)} MB`;
  }

  $: serverEnabled = serverMods.filter(m => m.enabled).length;
  $: userEnabled   = userMods.filter(m => m.enabled).length;

  // Textos del panel de resolución
  $: resolutionLines = (() => {
    if (!pendingResolution) return [];
    const lines: { icon: string; text: string; color: string }[] = [];
    const r = pendingResolution;
    if (r.depsToEnable.length > 0) {
      lines.push({
        icon: '🔗',
        text: `También activará: ${r.depsToEnable.map(modName).join(', ')}`,
        color: 'text-blue-400/80',
      });
    }
    if (r.conflicts.length > 0) {
      lines.push({
        icon: '⚡',
        text: `Desactivará: ${r.conflicts.map(modName).join(', ')} (incompatible)`,
        color: 'text-orange-400/80',
      });
    }
    if (r.dependents.length > 0) {
      lines.push({
        icon: '⚠️',
        text: `${r.dependents.map(modName).join(', ')} ${r.dependents.length === 1 ? 'depende' : 'dependen'} de este mod`,
        color: 'text-yellow-400/80',
      });
    }
    return lines;
  })();

  $: resolutionPrimaryLabel = (() => {
    if (!pendingResolution) return '';
    const r = pendingResolution;
    if (r.enabling) {
      if (r.depsToEnable.length > 0 && r.conflicts.length > 0) return 'Activar (resolver todo)';
      if (r.depsToEnable.length > 0) return 'Activar todo';
      if (r.conflicts.length > 0) return 'Activar y desactivar conflicto';
    } else {
      return 'Desactivar todo';
    }
    return 'Confirmar';
  })();

  $: resolutionSoloLabel = (() => {
    if (!pendingResolution) return '';
    return pendingResolution.enabling ? 'Solo este mod' : 'Solo este mod';
  })();

  // ¿Mostrar el botón "solo este"?
  // No mostrar si hay conflictos (activar dos mods incompatibles es siempre un problema)
  $: showSoloButton = pendingResolution !== null && pendingResolution.conflicts.length === 0;
</script>

<div in:fade={{ duration: 300 }} class="fixed inset-0 flex flex-col" style="background: var(--color-secondary)">

  <!-- Header -->
  <header data-tauri-drag-region
          class="flex items-center justify-between px-6 py-4 border-b border-white/10">
    <div class="flex items-center gap-3">
      <button on:click={() => screen.set('home')}
              class="text-white/50 hover:text-white transition-colors text-lg">←</button>
      <h1 class="font-bold text-white">Mods opcionales</h1>
    </div>

    <!-- Tab switcher -->
    <div class="flex items-center gap-1 rounded-lg p-1" style="background: rgba(255,255,255,0.07)">
      <button
        on:click={() => activeTab = 'server'}
        class="px-3 py-1 rounded-md text-sm font-medium transition-colors"
        style={activeTab === 'server'
          ? 'background: var(--color-primary); color: white'
          : 'color: rgba(255,255,255,0.5)'}>
        Del servidor {serverMods.length > 0 ? `(${serverEnabled}/${serverMods.length})` : ''}
      </button>
      <button
        on:click={() => activeTab = 'user'}
        class="px-3 py-1 rounded-md text-sm font-medium transition-colors"
        style={activeTab === 'user'
          ? 'background: var(--color-primary); color: white'
          : 'color: rgba(255,255,255,0.5)'}>
        Tus mods {userMods.length > 0 ? `(${userEnabled}/${userMods.length})` : ''}
      </button>
    </div>
  </header>

  <!-- ── SERVER MODS TAB ──────────────────────────────────────────────────── -->
  {#if activeTab === 'server'}
    <!-- Toolbar -->
    <div class="flex items-center justify-between px-6 py-3 border-b border-white/5">
      <p class="text-white/40 text-xs">
        Mods del servidor que puedes activar o desactivar.
        Los marcados con 📥 se descargarán en el próximo sync.
      </p>
      <button
        on:click={rebuild}
        disabled={rebuilding}
        title="Re-linkea todos los mods opcionales habilitados desde caché sin re-descargarlos"
        class="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium
               transition-colors text-white/60 hover:text-white disabled:opacity-40"
        style="background: rgba(255,255,255,0.07)">
        {rebuilding ? '⏳' : '🔧'} Reconstruir
      </button>
    </div>

    <div class="flex-1 overflow-y-auto p-6 flex flex-col gap-3">
      {#if loadingServer}
        <div class="flex justify-center py-20">
          <div class="w-8 h-8 border-2 border-white/20 border-t-white/80 rounded-full animate-spin"></div>
        </div>

      {:else if serverMods.length === 0}
        <div class="flex flex-col items-center justify-center py-16 gap-3 text-center">
          <div class="text-4xl opacity-30">📋</div>
          <p class="text-white/50 font-medium">Sin mods opcionales en el manifest</p>
          <p class="text-white/30 text-sm">El servidor no ha definido ningún mod opcional.</p>
        </div>

      {:else}
        {#each serverMods as mod (mod.id)}
          {@const isToggling = toggling.has(mod.id)}
          {@const hasPending = pendingResolution?.modId === mod.id}

          <div class="rounded-xl overflow-hidden transition-colors"
               style="background: rgba(255,255,255,0.05)">

            <!-- Fila principal del mod -->
            <div class="flex items-start gap-4 p-4">

              <!-- Icon -->
              {#if mod.iconUrl}
                <img src={mod.iconUrl} alt={mod.name}
                     class="w-10 h-10 rounded-lg object-cover flex-shrink-0" />
              {:else}
                <div class="w-10 h-10 rounded-lg flex-shrink-0 flex items-center justify-center"
                     style="background: rgba(255,255,255,0.08); font-size:1.2rem">🧩</div>
              {/if}

              <!-- Info -->
              <div class="flex-1 min-w-0">
                <div class="flex items-center gap-2 flex-wrap">
                  <span class="font-medium text-white text-sm">{mod.name}</span>
                  {#if mod.category}
                    <span class="text-xs px-2 py-0.5 rounded-full text-white/50"
                          style="background: rgba(255,255,255,0.1)">{mod.category}</span>
                  {/if}
                  <!-- Estado cache/linked -->
                  {#if mod.enabled}
                    {#if mod.linked}
                      <span class="text-xs text-green-400/70">✓ activo</span>
                    {:else if mod.inCache}
                      <span class="text-xs text-yellow-400/70">⚠ listo, usa Reconstruir</span>
                    {:else}
                      <span class="text-xs text-blue-400/70">📥 sync pendiente</span>
                    {/if}
                  {:else if mod.inCache}
                    <span class="text-xs text-white/30">⚡ en caché</span>
                  {/if}
                </div>

                {#if mod.description}
                  <p class="text-white/45 text-xs mt-1 line-clamp-2">{mod.description}</p>
                {/if}

                <!-- Deps y conflictos (descriptivos) -->
                {#if mod.dependsOn.length > 0}
                  <p class="text-white/30 text-xs mt-0.5">
                    🔗 Requiere: {mod.dependsOn.map(id => modName(id)).join(', ')}
                  </p>
                {/if}
                {#if mod.conflictsWith.length > 0}
                  <p class="text-orange-400/50 text-xs mt-0.5">
                    ⚡ Incompatible con: {mod.conflictsWith.map(id => modName(id)).join(', ')}
                  </p>
                {/if}
              </div>

              <!-- Toggle -->
              <button
                on:click={() => handleToggleClick(mod)}
                disabled={isToggling}
                aria-label={mod.enabled ? 'Desactivar' : 'Activar'}
                class="flex-shrink-0 w-11 h-6 rounded-full transition-colors relative disabled:opacity-50"
                style={mod.enabled
                  ? 'background: var(--color-primary)'
                  : hasPending
                    ? 'background: rgba(251,191,36,0.4)'
                    : 'background: rgba(255,255,255,0.15)'}>
                {#if isToggling}
                  <span class="absolute inset-0 flex items-center justify-center">
                    <span class="w-3 h-3 border border-white/60 border-t-white rounded-full animate-spin"></span>
                  </span>
                {:else}
                  <span class="absolute top-0.5 w-5 h-5 rounded-full bg-white shadow transition-all duration-200"
                        class:left-0.5={!mod.enabled}
                        class:left-5={mod.enabled}></span>
                {/if}
              </button>
            </div>

            <!-- Panel de resolución (aparece solo cuando hay dependencias/conflictos pendientes) -->
            {#if hasPending && pendingResolution}
              <div transition:slide={{ duration: 200 }}
                   class="border-t px-4 py-3 flex flex-col gap-2.5"
                   style="border-color: rgba(255,255,255,0.08); background: rgba(0,0,0,0.15)">

                <!-- Líneas de info -->
                {#each resolutionLines as line}
                  <p class="text-xs {line.color}">{line.icon} {line.text}</p>
                {/each}

                <!-- Botones de acción -->
                <div class="flex items-center gap-2 pt-0.5">
                  <!-- Acción principal: resolver todo -->
                  <button
                    on:click={() => applyResolution(true)}
                    class="px-3 py-1 rounded-md text-xs font-medium text-white transition-colors"
                    style="background: var(--color-primary)">
                    {resolutionPrimaryLabel}
                  </button>

                  <!-- Solo este (sin resolver deps/conflictos) -->
                  {#if showSoloButton}
                    <button
                      on:click={() => applyResolution(false)}
                      class="px-3 py-1 rounded-md text-xs font-medium transition-colors text-white/60 hover:text-white"
                      style="background: rgba(255,255,255,0.08)"
                      title={pendingResolution.enabling
                        ? 'Activa el mod sin activar sus dependencias (puede no funcionar)'
                        : 'Desactiva solo este mod sin tocar los que dependen de él'}>
                      {resolutionSoloLabel}
                    </button>
                  {/if}

                  <!-- Cancelar -->
                  <button
                    on:click={() => pendingResolution = null}
                    class="px-3 py-1 rounded-md text-xs transition-colors text-white/40 hover:text-white/70">
                    Cancelar
                  </button>
                </div>
              </div>
            {/if}
          </div>
        {/each}
      {/if}
    </div>

  <!-- ── USER MODS TAB ─────────────────────────────────────────────────────── -->
  {:else}
    <!-- Toolbar -->
    <div class="flex items-center justify-between px-6 py-3 border-b border-white/5">
      <p class="text-white/40 text-xs">
        Tus propios <span class="font-mono text-white/60">.jar</span> en
        <span class="font-mono text-white/60">mods-optional/</span>.
        El servidor no los toca.
      </p>
      <div class="flex gap-2">
        <button on:click={loadUserMods}
                class="px-3 py-1.5 rounded-lg text-xs font-medium text-white/60 hover:text-white transition-colors"
                style="background: rgba(255,255,255,0.07)">
          🔄 Refrescar
        </button>
        <button on:click={openUserFolder}
                class="px-3 py-1.5 rounded-lg text-xs font-medium text-white/60 hover:text-white transition-colors"
                style="background: rgba(255,255,255,0.07)">
          📁 Abrir carpeta
        </button>
      </div>
    </div>

    <div class="flex-1 overflow-y-auto p-6 flex flex-col gap-3">
      {#if loadingUser}
        <div class="flex justify-center py-20">
          <div class="w-8 h-8 border-2 border-white/20 border-t-white/80 rounded-full animate-spin"></div>
        </div>

      {:else if userMods.length === 0}
        <div class="flex flex-col items-center justify-center py-16 gap-4 text-center">
          <div class="text-5xl opacity-30">📦</div>
          <div>
            <p class="text-white/60 font-medium">Carpeta vacía</p>
            <p class="text-white/35 text-sm mt-1">
              Abre la carpeta y añade archivos <span class="font-mono">.jar</span>.<br>
              Luego pulsa Refrescar para verlos aquí.
            </p>
          </div>
          <button on:click={openUserFolder}
                  class="mt-1 px-4 py-2 rounded-lg text-sm font-medium text-white"
                  style="background: var(--color-primary)">
            📁 Abrir carpeta mods-optional
          </button>
        </div>

      {:else}
        {#each userMods as mod (mod.filename)}
          {@const isToggling = toggling.has(mod.filename)}
          <div class="flex items-center gap-4 p-4 rounded-xl"
               style="background: rgba(255,255,255,0.05)">
            <div class="w-10 h-10 rounded-lg flex-shrink-0 flex items-center justify-center"
                 style="background: rgba(255,255,255,0.08); font-size:1.2rem">🧩</div>
            <div class="flex-1 min-w-0">
              <p class="font-medium text-white text-sm truncate" title={mod.filename}>{mod.filename}</p>
              <p class="text-white/35 text-xs mt-0.5">{formatSize(mod.sizeBytes)}</p>
            </div>
            {#if mod.enabled}
              <span class="text-xs px-2 py-0.5 rounded-full font-medium"
                    style="background: rgba(34,197,94,0.15); color: rgb(134,239,172)">activo</span>
            {/if}
            <button
              on:click={() => toggleUser(mod)}
              disabled={isToggling}
              aria-label={mod.enabled ? 'Desactivar' : 'Activar'}
              class="flex-shrink-0 w-11 h-6 rounded-full transition-colors relative disabled:opacity-50"
              style={mod.enabled ? 'background: var(--color-primary)' : 'background: rgba(255,255,255,0.15)'}>
              {#if isToggling}
                <span class="absolute inset-0 flex items-center justify-center">
                  <span class="w-3 h-3 border border-white/60 border-t-white rounded-full animate-spin"></span>
                </span>
              {:else}
                <span class="absolute top-0.5 w-5 h-5 rounded-full bg-white shadow transition-all duration-200"
                      class:left-0.5={!mod.enabled}
                      class:left-5={mod.enabled}></span>
              {/if}
            </button>
          </div>
        {/each}
      {/if}
    </div>
  {/if}

</div>
