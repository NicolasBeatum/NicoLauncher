<script lang="ts">
  import { onMount, onDestroy, tick } from 'svelte';
  import { fade } from 'svelte/transition';
  import { marked } from 'marked';
  import { branding } from '$lib/stores/branding';
  import { session, logout } from '$lib/stores/auth';
  import { screen } from '$lib/stores/screen';
  import { addToast } from '$lib/stores/toast';
  import { api, events, type ServerManifestDto, type SyncPlanDto, type ProgressEvent, type InstanceDto, type MissingModDto, type ServerStatusDto } from '$lib/tauri';
  import { open } from '@tauri-apps/plugin-shell';

  // Configure marked: safe, no pedantic
  marked.setOptions({ async: false });

  type PlayState = 'idle' | 'syncing' | 'launching' | 'running';

  let playState: PlayState = 'idle';
  let manifest: ServerManifestDto | null = null;
  let syncPlan: SyncPlanDto | null = null;
  let progress: ProgressEvent | null = null;
  let appLogs: string[] = [];
  let showLogs = false;
  type LogFilter = 'all' | 'errors' | 'game';
  let logFilter: LogFilter = 'all';

  let manifestLoading = true;
  let manifestError: string | null = null;
  let unlisteners: (() => void)[] = [];
  let pollInterval: ReturnType<typeof setInterval> | null = null;

  // ── Server status ───────────────────────────────────────────────────────────
  let serverStatus: ServerStatusDto | null = null;
  let statusInterval: ReturnType<typeof setInterval> | null = null;
  let copyIpLabel = 'Copiar IP';

  async function refreshServerStatus() {
    try { serverStatus = await api.serverStatus(); }
    catch { serverStatus = null; }
  }

  async function copyIp() {
    const addr = $branding.serverAddress;
    if (!addr) return;
    const full = $branding.serverPort !== 25565
      ? `${addr}:${$branding.serverPort}`
      : addr;
    try {
      await navigator.clipboard.writeText(full);
      copyIpLabel = '✓ Copiado';
      setTimeout(() => { copyIpLabel = 'Copiar IP'; }, 2000);
    } catch {
      addToast('error', 'No se pudo copiar la IP');
    }
  }

  // ── Modal mods faltantes ────────────────────────────────────────────────────
  let missingMods: MissingModDto[] = [];
  let modsToRestore: string[] = [];       // IDs marcados para restaurar
  let missingModalResolve: ((ids: string[]) => void) | null = null;

  /** Detecta mods borrados del disco y muestra el modal si los hay.
   *  Retorna los IDs que el usuario eligió restaurar (vacío = continuar sin ellos). */
  async function checkMissingMods(): Promise<string[]> {
    try {
      missingMods = await api.syncCheckMissing();
    } catch {
      return [];
    }
    if (missingMods.length === 0) return [];

    // Pre-marcar todos para restaurar (el usuario puede desmarcar individualmente)
    modsToRestore = missingMods.map(m => m.id);

    return new Promise(resolve => {
      missingModalResolve = resolve;
    });
  }

  function confirmRestore() {
    missingModalResolve?.(modsToRestore);
    missingModalResolve = null;
    missingMods = [];
  }

  function skipRestore() {
    missingModalResolve?.([]);
    missingModalResolve = null;
    missingMods = [];
  }

  // ── Instancias ──────────────────────────────────────────────────────────────
  let instances: InstanceDto[] = [];
  let switchingInstance = false;

  async function loadInstances() {
    // Intentar refrescar desde el registry remoto (si está configurado en el servidor).
    // Fallo silencioso — si no hay URL o el servidor está caído, usamos las del config.
    try { await api.refreshInstancesRegistry(); } catch { /* ignorar */ }
    try { instances = await api.getInstances(); } catch { /* ignorar */ }
  }

  async function switchInstance(id: string) {
    if (switchingInstance || instances.find(i => i.id === id)?.isActive) return;
    switchingInstance = true;
    playState = 'idle';
    manifest = null;
    syncPlan = null;
    appLogs = [];
    clearPoll();
    try {
      await api.setActiveInstance(id);
      instances = instances.map(i => ({ ...i, isActive: i.id === id }));
      await loadManifest();
    } catch (e) {
      addToast('error', String(e));
    } finally {
      switchingInstance = false;
    }
  }

  onMount(async () => {
    await loadInstances();
    await loadManifest();

    // Server status — fetch immediately and every 30s
    if ($branding.serverAddress) {
      refreshServerStatus();
      statusInterval = setInterval(refreshServerStatus, 30_000);
    }

    // Registrar listener de progreso una vez, para toda la vida del componente.
    // (Hacerlo dentro de handlePlay con await causa un hang en Tauri 2.)
    const unlistenProgress = await events.onProgress(e => {
      // Actualizar progress si viene cualquier campo de estado
      if (e.stage != null || e.current != null || e.total != null) {
        progress = {
          stage:   e.stage   ?? progress?.stage,
          current: e.current ?? progress?.current,
          total:   e.total   ?? progress?.total,
          message: e.message ?? null,
        } as typeof progress;
      }
      if (e.message) {
        appLogs = [...appLogs.slice(-499), e.message];
        tick().then(scrollConsole);
      }
    });
    unlisteners.push(unlistenProgress);

    // Auto-check para actualizaciones del launcher (silencioso si no está configurado)
    try {
      const update = await api.checkUpdate();
      if (update?.available) {
        addToast('info', `Nueva versión disponible: v${update.version} — ve a Ajustes`);
      }
    } catch { /* ignorar si el updater no está configurado */ }

    // Background manifest refresh cada 5 minutos
    manifestRefreshInterval = setInterval(async () => {
      if (playState !== 'idle') return; // no interrumpir durante sync/launch
      try {
        const fresh = await api.manifestFetch();
        if (fresh && manifest && fresh.manifestVersion !== manifest.manifestVersion) {
          manifest = fresh;
          syncPlan = await api.syncComputePlan();
          addToast('info', `📋 Manifest actualizado a v${fresh.manifestVersion}`);
          offlineMode = false;
        }
      } catch { /* ignorar errores de red en el refresh de fondo */ }
    }, 5 * 60 * 1000);
  });

  let manifestRefreshInterval: ReturnType<typeof setInterval> | null = null;
  let offlineMode = false;

  onDestroy(() => {
    unlisteners.forEach(u => u());
    clearPoll();
    if (manifestRefreshInterval) clearInterval(manifestRefreshInterval);
    if (statusInterval) clearInterval(statusInterval);
  });

  async function loadManifest() {
    manifestLoading = true;
    manifestError = null;
    offlineMode = false;
    try {
      manifest = await api.manifestFetch();
      syncPlan = await api.syncComputePlan();
    } catch (e) {
      const msg = String(e);
      // El backend devuelve "__offline__:<version>" cuando carga desde caché de disco
      if (msg.startsWith('__offline__:')) {
        offlineMode = true;
        addToast('info', '📦 Sin conexión — usando manifest en caché');
        // Con manifest en caché el estado ya está cargado en Rust, solo pedimos el plan
        try { syncPlan = await api.syncComputePlan(); } catch { /* ignorar */ }
      } else {
        manifestError = msg;
      }
    } finally {
      manifestLoading = false;
    }
  }

  async function handlePlay() {
    if (playState === 'running') return;

    // 0. Refrescar el plan justo antes de jugar para capturar cambios externos
    //    (forzar sync, mods borrados, etc.) sin depender del reactive $:
    try { syncPlan = await api.syncComputePlan(); } catch { /* ignorar */ }

    // 1. Detectar mods borrados del disco y preguntar al usuario qué hacer
    const restoreMods = await checkMissingMods();

    // 2. Hay sync si: el usuario quiere restaurar mods O el plan lo requiere
    const planNeedsSync = syncPlan && (
      syncPlan.modsToDownload > 0 || syncPlan.optionalModsToDownload > 0 ||
      syncPlan.modsToRemove > 0 ||
      syncPlan.configsToApply > 0 || syncPlan.loaderAction !== 'none'
    );
    const needsSync = restoreMods.length > 0 || planNeedsSync;

    if (needsSync) {
      playState = 'syncing';
      progress = null;
      showLogs = true;
      appLogs = [...appLogs, '🔄 Sincronizando mods…'];

      try {
        await api.syncApply(restoreMods);
        syncPlan = await api.syncComputePlan();
        appLogs = [...appLogs, '✅ Sync completado'];
      } catch (e) {
        addToast('error', `Error de sincronización: ${e}`);
        playState = 'idle';
        return;
      }
    }

    playState = 'launching';
    appLogs = [...appLogs, '▶ Iniciando Minecraft…'];
    showLogs = true;

    try {
      await api.launchGame();
    } catch (e) {
      addToast('error', `Error al lanzar: ${e}`);
      playState = 'idle';
      return;
    }

    // Pollear estado del launch cada 300 ms
    pollInterval = setInterval(async () => {
      try {
        const status = await api.getLaunchStatus();

        if (status.logs.length > 0) {
          appLogs = [...appLogs.slice(-499), ...status.logs];
          await tick();
          scrollConsole();
        }

        if (status.error) {
          clearPoll();
          addToast('error', `Error al lanzar: ${status.error}`);
          playState = 'idle';
        } else if (status.exitCode !== null && status.exitCode !== undefined) {
          clearPoll();
          const ok = status.exitCode === 0;
          addToast(ok ? 'success' : 'error',
            `Minecraft cerrado (código ${status.exitCode})`);
          playState = 'idle';
        } else if (status.started && playState === 'launching') {
          playState = 'running';
        }
      } catch { /* ignorar errores de red durante polling */ }
    }, 300);
  }

  async function handleKill() {
    try {
      await api.gameKill();
      addToast('info', 'Señal de cierre enviada a Minecraft');
    } catch (e) {
      addToast('error', String(e));
    }
  }

  async function handleLogout() {
    await logout();
    screen.set('login');
  }

  function clearPoll() {
    if (pollInterval) { clearInterval(pollInterval); pollInterval = null; }
  }

  function scrollConsole() {
    const el = document.getElementById('console-scroll');
    if (el) el.scrollTop = el.scrollHeight;
  }

  // Detalles del sync plan legibles
  function syncSummary(plan: SyncPlanDto): string {
    const parts: string[] = [];
    if (plan.modsToDownload > 0) parts.push(`${plan.modsToDownload} mod${plan.modsToDownload !== 1 ? 's' : ''} nuevo${plan.modsToDownload !== 1 ? 's' : ''}`);
    if (plan.optionalModsToDownload > 0) parts.push(`${plan.optionalModsToDownload} opcional${plan.optionalModsToDownload !== 1 ? 'es' : ''}`);
    if (plan.modsToRemove   > 0) parts.push(`${plan.modsToRemove} a eliminar`);
    if (plan.configsToApply > 0) parts.push(`${plan.configsToApply} config`);
    if (plan.loaderAction !== 'none') parts.push(`loader: ${plan.loaderAction}`);
    return parts.join(' · ');
  }

  // Refrescar syncPlan cada vez que el usuario vuelve a esta pantalla
  // (Home queda montado con display:none — onMount no vuelve a correr)
  $: if ($screen === 'home' && playState === 'idle') {
    api.syncComputePlan()
      .then(plan => { syncPlan = plan; })
      .catch(() => {});
  }

  $: syncNeeded = syncPlan && (
    syncPlan.modsToDownload > 0 || syncPlan.optionalModsToDownload > 0 ||
    syncPlan.modsToRemove > 0 ||
    syncPlan.configsToApply > 0 || syncPlan.loaderAction !== 'none'
  );

  $: progressPercent = progress?.total
    ? Math.round(((progress.current ?? 0) / progress.total) * 100)
    : 0;

  $: filteredLogs = logFilter === 'all'    ? appLogs
                  : logFilter === 'errors' ? appLogs.filter(l => l.startsWith('ERROR'))
                  :                          appLogs.filter(l => l.startsWith('[MC]'));

  $: errorCount = appLogs.filter(l => l.startsWith('ERROR')).length;

  // Rendered Markdown for the announcement body
  $: announcementHtml = manifest?.announcementBody
    ? (marked.parse(manifest.announcementBody) as string)
    : '';

  // Full server address string (with port if non-default)
  $: serverAddressFull = $branding.serverAddress
    ? ($branding.serverPort !== 25565
        ? `${$branding.serverAddress}:${$branding.serverPort}`
        : $branding.serverAddress)
    : '';
</script>

<div in:fade={{ duration: 300 }} class="fixed inset-0 flex flex-col" style="background: var(--color-secondary)">
  <!-- Background art overlay -->
  <div class="absolute inset-0 opacity-10"
       style="background: radial-gradient(ellipse at 50% -20%, var(--color-primary) 0%, transparent 60%)">
  </div>

  <!-- Title bar / header -->
  <header data-tauri-drag-region
          class="relative z-10 flex items-center justify-between px-6 py-4 border-b border-white/10">
    <div class="flex items-center gap-3">
      <div class="w-8 h-8 rounded-lg flex items-center justify-center text-sm font-bold text-white"
           style="background: var(--color-primary)">
        {$branding.displayName.charAt(0)}
      </div>

      <!-- Selector de instancias (solo visible si hay más de una) -->
      {#if instances.length > 1}
        <div class="flex items-center gap-1 ml-2">
          {#each instances as inst}
            {@const active = inst.isActive}
            <button
              on:click={() => switchInstance(inst.id)}
              disabled={switchingInstance || active}
              title={inst.description || inst.displayName}
              class="px-3 py-1 rounded-lg text-xs font-semibold transition-all
                     disabled:cursor-default"
              style={active
                ? `background: ${inst.color || 'var(--color-primary)'}; color: white;`
                : 'background: rgba(255,255,255,0.07); color: rgba(255,255,255,0.5);'}>
              {inst.displayName}
            </button>
          {/each}
        </div>
      {/if}
      <span class="font-semibold text-white text-sm">{$branding.displayName}</span>
    </div>

    <div class="flex items-center gap-4">
      {#if $session}
        <span class="text-white/60 text-sm">{$session.username}</span>
        <button on:click={handleLogout}
                class="text-white/40 hover:text-white/80 text-xs transition-colors">
          Cerrar sesión
        </button>
      {/if}
      <button on:click={() => screen.set('settings')}
              class="text-white/60 hover:text-white text-lg transition-colors" title="Ajustes">⚙</button>
    </div>
  </header>

  <!-- Main content -->
  <main class="relative z-10 flex-1 flex flex-col items-center justify-center gap-6 px-4">

    <!-- Manifest info -->
    {#if manifest}
      <p class="text-white/40 text-sm flex items-center gap-2">
        {manifest.mcVersion}
        {#if manifest.loaderType} · {manifest.loaderType} {manifest.loaderVersion}{/if}
        · v{manifest.manifestVersion}
        {#if offlineMode}
          <span class="text-yellow-400/80 text-xs font-medium">📦 sin conexión</span>
        {/if}
      </p>
    {/if}

    <!-- ── Play area ──────────────────────────────────────────── -->
    <div class="flex flex-col items-center gap-3 w-full max-w-xs">

      {#if playState === 'syncing'}
        <!-- Sync progress -->
        <div class="w-full flex flex-col items-center gap-2">
          <!-- Stage label -->
          <p class="text-white/80 text-sm font-medium">
            {progress?.stage ?? 'Preparando sincronización…'}
          </p>

          <!-- Barra de progreso o indeterminada -->
          {#if progress?.total}
            <div class="w-full h-2 rounded-full bg-white/10 overflow-hidden">
              <div class="h-full rounded-full transition-all duration-200"
                   style="width: {progressPercent}%; background: var(--color-primary)"></div>
            </div>
            <div class="flex justify-between w-full mt-0.5">
              <span class="text-white/40 text-xs">{progressPercent}%</span>
              <span class="text-white/40 text-xs">{progress.current ?? 0} / {progress.total}</span>
            </div>
          {:else}
            <!-- Barra indeterminada mientras no hay total -->
            <div class="w-full h-2 rounded-full bg-white/10 overflow-hidden">
              <div class="h-full w-full rounded-full opacity-60 animate-pulse"
                   style="background: var(--color-primary)"></div>
            </div>
          {/if}

          <!-- Archivo actual -->
          {#if progress?.message}
            <p class="text-white/35 text-xs font-mono truncate w-full text-center leading-snug">
              {progress.message}
            </p>
          {/if}
        </div>

      {:else if playState === 'launching'}
        <div class="flex flex-col items-center gap-3">
          <div class="w-10 h-10 border-2 border-white/20 border-t-white/80 rounded-full animate-spin"></div>
          <p class="text-white/60 text-sm">{appLogs[appLogs.length - 1] ?? 'Iniciando Minecraft…'}</p>
        </div>

      {:else if manifestLoading}
        <div class="flex flex-col items-center gap-3">
          <div class="w-10 h-10 border-2 border-white/20 border-t-white/80 rounded-full animate-spin"></div>
          <p class="text-white/40 text-sm">Cargando manifest…</p>
        </div>

      {:else if manifestError}
        <!-- Manifest error: inline con botón de retry -->
        <div class="w-full p-4 rounded-xl text-center space-y-3" style="background: rgba(239,68,68,0.1); border: 1px solid rgba(239,68,68,0.3)">
          <p class="text-red-400 text-sm font-medium">⚠ Error al cargar el manifest</p>
          <p class="text-white/40 text-xs font-mono leading-relaxed">{manifestError}</p>
          <button on:click={loadManifest}
                  class="px-4 py-1.5 rounded-lg text-sm font-medium text-white transition-all"
                  style="background: var(--color-primary)">
            Reintentar
          </button>
        </div>

        <!-- Aún así se puede jugar sin manifest (modo offline/dev) -->
        <button on:click={handlePlay}
                class="text-white/30 hover:text-white/60 text-xs transition-colors underline underline-offset-2">
          Jugar sin manifest (modo dev)
        </button>

      {:else}
        <!-- PLAY button -->
        <button
          on:click={handlePlay}
          disabled={playState === 'running'}
          class="w-full py-5 rounded-2xl font-bold text-xl text-white
                 transition-all duration-200 hover:scale-105 active:scale-95 shadow-2xl
                 disabled:cursor-default disabled:hover:scale-100"
          style="background: var(--color-primary)"
        >
          {#if playState === 'running'}
            🟢 Minecraft está corriendo
          {:else if syncNeeded}
            ▶ JUGAR
          {:else}
            ▶ JUGAR
          {/if}
        </button>

        <!-- Sync details hint -->
        {#if syncNeeded && syncPlan}
          <p class="text-white/40 text-xs text-center">
            Sync necesario: {syncSummary(syncPlan)}
          </p>
        {/if}
      {/if}

      <!-- Botón Detener (solo cuando el juego corre) -->
      {#if playState === 'running'}
        <button on:click={handleKill}
                class="px-5 py-2 rounded-xl text-sm font-medium transition-all
                       border border-red-500/40 text-red-400 hover:bg-red-500/10">
          ⏹ Detener Minecraft
        </button>
      {/if}

    </div>

    <!-- ── Info bar: announcement + server status ──────────────────────────── -->
    {#if manifest?.announcementTitle || $branding.serverAddress}
      <div class="w-full max-w-2xl flex gap-4 px-2">

        <!-- Novedades / announcement -->
        {#if manifest?.announcementTitle}
          <div class="flex-1 px-4 py-3 rounded-xl text-sm"
               style="background: rgba(255,255,255,0.06)">
            <p class="font-semibold text-white mb-1.5">
              📰 {manifest.announcementTitle}
            </p>
            {#if announcementHtml}
              <div class="prose-sm text-white/55 leading-relaxed [&_a]:text-blue-400 [&_strong]:text-white/80 [&_ul]:list-disc [&_ul]:pl-4 [&_p]:mb-1">
                {@html announcementHtml}
              </div>
            {/if}
          </div>
        {/if}

        <!-- Estado del servidor -->
        {#if $branding.serverAddress}
          <div class="flex-shrink-0 w-44 px-4 py-3 rounded-xl flex flex-col gap-1.5"
               style="background: rgba(255,255,255,0.06)">

            <!-- Online / offline indicator -->
            <div class="flex items-center gap-1.5">
              {#if serverStatus === null}
                <span class="w-2 h-2 rounded-full bg-white/20 animate-pulse"></span>
                <span class="text-white/30 text-xs">Comprobando…</span>
              {:else if serverStatus.online}
                <span class="w-2 h-2 rounded-full bg-green-400 shadow-[0_0_6px_rgba(74,222,128,0.8)]"></span>
                <span class="text-green-400 text-xs font-medium">En línea</span>
              {:else}
                <span class="w-2 h-2 rounded-full bg-red-400/70"></span>
                <span class="text-white/40 text-xs">Sin conexión</span>
              {/if}
            </div>

            <!-- Jugadores + ping -->
            {#if serverStatus?.online}
              {#if serverStatus.playersOnline !== null && serverStatus.playersMax !== null}
                <p class="text-white/60 text-xs">
                  👥 {serverStatus.playersOnline}/{serverStatus.playersMax}
                </p>
              {/if}
              {#if serverStatus.pingMs !== null}
                <p class="text-white/40 text-xs">
                  {serverStatus.pingMs}ms
                </p>
              {/if}
              {#if serverStatus.motd}
                <p class="text-white/30 text-xs truncate" title={serverStatus.motd}>
                  {serverStatus.motd}
                </p>
              {/if}
            {/if}

            <!-- Dirección + botón Copiar IP -->
            <div class="mt-auto pt-1 border-t border-white/5">
              <p class="text-white/30 text-xs font-mono truncate mb-1" title={serverAddressFull}>
                {serverAddressFull}
              </p>
              <button
                on:click={copyIp}
                class="w-full py-1 rounded-lg text-xs font-medium transition-colors
                       text-white/60 hover:text-white"
                style="background: rgba(255,255,255,0.08)">
                {copyIpLabel}
              </button>
            </div>
          </div>
        {/if}

      </div>
    {/if}

  </main>

  <!-- Console -->
  <div class="relative z-10 border-t border-white/10" style="background: rgba(0,0,0,0.4)">
    <!-- Console header: toggle + filtros + limpiar -->
    <div class="flex items-center px-4 py-1 gap-3">
      <button on:click={() => showLogs = !showLogs}
              class="flex items-center gap-1.5 text-xs text-white/40 hover:text-white/70 transition-colors flex-1 text-left font-mono">
        <span>Consola</span>
        {#if appLogs.length > 0}<span class="text-white/25">({appLogs.length})</span>{/if}
        {#if errorCount > 0}<span class="text-red-400/80 ml-1">⚠ {errorCount}</span>{/if}
        <span class="ml-auto">{showLogs ? '▼' : '▲'}</span>
      </button>
    </div>

    {#if showLogs}
      <!-- Filter tabs + clear -->
      <div class="flex items-center gap-1 px-4 pb-1 border-b border-white/5">
        {#each [['all','Todo'],['errors','Errores'],['game','Juego']] as [val, label] (val)}
          <button on:click={() => logFilter = val as LogFilter}
                  class="px-2 py-0.5 rounded text-xs transition-colors {logFilter === val ? 'text-white' : 'text-white/30 hover:text-white/60'}"
                  style={logFilter === val ? 'background: var(--color-primary)' : ''}>
            {label}
          </button>
        {/each}
        <button on:click={() => { appLogs = []; logFilter = 'all'; }}
                class="ml-auto text-white/25 hover:text-white/50 text-xs transition-colors px-2">
          Limpiar
        </button>
      </div>

      <!-- Log lines -->
      <div class="h-36 overflow-y-auto px-4 py-2 font-mono text-xs space-y-0.5"
           id="console-scroll">
        {#if filteredLogs.length === 0}
          <p class="text-white/20 italic">
            {appLogs.length === 0 ? 'Sin logs aún…' : 'No hay entradas en este filtro.'}
          </p>
        {:else}
          {#each filteredLogs as line}
            <p class={
              line.startsWith('ERROR') ? 'text-red-400' :
              line.startsWith('[MC]')  ? 'text-white/40' :
              'text-green-400/80'
            }>{line}</p>
          {/each}
        {/if}
      </div>
    {/if}
  </div>

  <!-- ── Modal: mods eliminados del disco ─────────────────────────────────── -->
  {#if missingMods.length > 0}
    <div class="fixed inset-0 z-50 flex items-center justify-center p-4"
         style="background: rgba(0,0,0,0.7)">
      <div class="rounded-2xl p-6 w-full max-w-sm shadow-2xl"
           style="background: var(--color-secondary); border: 1px solid rgba(255,255,255,0.1)">

        <h2 class="font-bold text-white mb-1">Mods eliminados detectados</h2>
        <p class="text-white/50 text-sm mb-4">
          Los siguientes mods no están en tu carpeta. ¿Quieres restaurarlos?
        </p>

        <!-- Lista con checkboxes -->
        <div class="space-y-1.5 mb-5 max-h-52 overflow-y-auto pr-1">
          {#each missingMods as mod (mod.id)}
            <label class="flex items-center gap-3 p-2.5 rounded-lg cursor-pointer transition-colors"
                   style="background: rgba(255,255,255,0.05)">
              <input
                type="checkbox"
                bind:group={modsToRestore}
                value={mod.id}
                class="w-4 h-4 rounded"
                style="accent-color: var(--color-primary)"
              />
              <div class="min-w-0">
                <p class="text-white text-sm font-medium truncate">{mod.name}</p>
                <p class="text-white/30 text-xs font-mono truncate">{mod.filename}</p>
              </div>
            </label>
          {/each}
        </div>

        <!-- Seleccionar / deseleccionar todos -->
        <div class="flex gap-2 mb-4 text-xs">
          <button on:click={() => modsToRestore = missingMods.map(m => m.id)}
                  class="text-white/40 hover:text-white/70 transition-colors">
            Seleccionar todos
          </button>
          <span class="text-white/20">·</span>
          <button on:click={() => modsToRestore = []}
                  class="text-white/40 hover:text-white/70 transition-colors">
            Deseleccionar todos
          </button>
        </div>

        <!-- Acciones -->
        <div class="flex gap-2">
          <button
            on:click={confirmRestore}
            disabled={modsToRestore.length === 0}
            class="flex-1 py-2.5 rounded-xl text-sm font-semibold text-white transition-all
                   disabled:opacity-40 disabled:cursor-default"
            style="background: var(--color-primary)">
            ↓ Restaurar {modsToRestore.length > 0 ? `(${modsToRestore.length})` : ''}
          </button>
          <button
            on:click={skipRestore}
            class="flex-1 py-2.5 rounded-xl text-sm font-semibold transition-all
                   border border-white/15 text-white/60 hover:text-white hover:border-white/30">
            Continuar sin ellos
          </button>
        </div>

      </div>
    </div>
  {/if}

  <!-- Footer nav -->
  <footer class="relative z-10 flex items-center justify-center gap-6 px-6 py-4 border-t border-white/10">
    <button on:click={() => screen.set('optional-mods')}
            class="text-white/50 hover:text-white text-sm transition-colors">
      Mods opcionales
    </button>
    <button on:click={() => screen.set('settings')}
            class="text-white/50 hover:text-white text-sm transition-colors">
      Ajustes
    </button>
    {#if $branding.discord}
      <button on:click={() => open($branding.discord)}
              class="text-white/50 hover:text-white text-sm transition-colors">
        Discord
      </button>
    {/if}
    {#if $branding.website}
      <button on:click={() => open($branding.website)}
              class="text-white/50 hover:text-white text-sm transition-colors">
        Web
      </button>
    {/if}
  </footer>
</div>
