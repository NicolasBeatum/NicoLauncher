<script lang="ts">
  import { onMount } from 'svelte';
  import { fade } from 'svelte/transition';
  import { screen } from '$lib/stores/screen';
  import { addToast } from '$lib/stores/toast';
  import { api, type SettingsDto, type UpdateInfoDto } from '$lib/tauri';

  let s: SettingsDto | null = null;
  let saving = false;

  // ── Updater ────────────────────────────────────────────────────────────────
  let checkingUpdate = false;
  let updateInfo: UpdateInfoDto | null = null;
  let installing = false;
  let installLogs: string[] = [];
  let installDone = false;
  let pollInterval: ReturnType<typeof setInterval> | null = null;

  async function checkUpdate() {
    checkingUpdate = true;
    updateInfo = null;
    try {
      updateInfo = await api.checkUpdate();
      if (!updateInfo) {
        addToast('info', 'El updater no está configurado en este launcher.');
      } else if (!updateInfo.available) {
        addToast('success', `Ya tienes la última versión (${updateInfo.currentVersion}).`);
      }
    } catch (e) {
      addToast('error', String(e));
    } finally {
      checkingUpdate = false;
    }
  }

  async function installUpdate() {
    installing = true;
    installLogs = [];
    installDone = false;
    try {
      await api.installUpdate();
      pollInterval = setInterval(async () => {
        const status = await api.getUpdateStatus();
        installLogs = [...installLogs, ...status.logs];
        if (status.done) {
          installDone = true;
          clearPoll();
        }
        if (status.error) {
          addToast('error', status.error);
          installing = false;
          clearPoll();
        }
      }, 400);
    } catch (e) {
      addToast('error', String(e));
      installing = false;
    }
  }

  function clearPoll() {
    if (pollInterval) { clearInterval(pollInterval); pollInterval = null; }
  }

  // ── Config overrides ───────────────────────────────────────────────────────
  let resettingOptions = false;

  async function resetOptions() {
    resettingOptions = true;
    try {
      await api.resetConfigOverride('options.txt');
      addToast('success', 'options.txt eliminado — se restaurará en el próximo sync');
    } catch (e) {
      addToast('error', String(e));
    } finally {
      resettingOptions = false;
    }
  }

  let forcingSyncReset = false;

  async function forceSyncReset() {
    forcingSyncReset = true;
    try {
      await api.syncForceReset();
      addToast('success', 'Estado de sync limpiado — pulsa PLAY para re-descargar todo');
    } catch (e) {
      addToast('error', String(e));
    } finally {
      forcingSyncReset = false;
    }
  }

  // ── Diagnostics ────────────────────────────────────────────────────────────
  let creatingReport = false;

  async function createReport() {
    creatingReport = true;
    try {
      const path = await api.createDiagnosticsReport();
      addToast('success', `Reporte creado: ${path.split(/[\\/]/).pop()}`);
    } catch (e) {
      addToast('error', String(e));
    } finally {
      creatingReport = false;
    }
  }

  onMount(async () => {
    try { s = await api.settingsGet(); }
    catch (e) { addToast('error', String(e)); }
  });

  async function save() {
    if (!s) return;
    saving = true;
    try {
      await api.settingsSet(s);
      addToast('success', 'Ajustes guardados');
    } catch (e) {
      addToast('error', String(e));
    } finally {
      saving = false;
    }
  }

  $: ramGB = s ? (s.ramMb / 1024).toFixed(1) : '0';
</script>

<div in:fade={{ duration: 300 }} class="fixed inset-0 flex flex-col" style="background: var(--color-secondary)">

  <!-- Header -->
  <header data-tauri-drag-region
          class="flex items-center justify-between px-6 py-4 border-b border-white/10">
    <div class="flex items-center gap-3">
      <button on:click={() => screen.set('home')}
              class="text-white/50 hover:text-white transition-colors text-lg">←</button>
      <h1 class="font-bold text-white">Ajustes</h1>
    </div>
    <button on:click={save} disabled={saving || !s}
            class="px-4 py-1.5 rounded-lg text-sm font-medium text-white transition-all
                   disabled:opacity-50"
            style="background: var(--color-primary)">
      {saving ? 'Guardando…' : 'Guardar'}
    </button>
  </header>

  <div class="flex-1 overflow-y-auto p-6 max-w-lg mx-auto w-full">
    {#if !s}
      <div class="flex justify-center py-20">
        <div class="w-8 h-8 border-2 border-white/20 border-t-white/80 rounded-full animate-spin"></div>
      </div>
    {:else}

      <!-- RAM -->
      {#if s.allowRamConfig}
        <section class="mb-8">
          <h2 class="text-white/60 text-xs font-semibold uppercase tracking-wider mb-3">Memoria RAM</h2>
          <div class="p-4 rounded-xl" style="background: rgba(255,255,255,0.05)">
            <div class="flex justify-between items-center mb-2">
              <span class="text-white text-sm">RAM asignada</span>
              <span class="font-mono text-sm" style="color: var(--color-accent)">{ramGB} GB</span>
            </div>
            <input
              type="range"
              min={s.ramMinMb}
              max={s.ramMaxMb}
              step="512"
              bind:value={s.ramMb}
              class="w-full accent-primary"
              style="accent-color: var(--color-primary)"
            />
            <div class="flex justify-between text-white/30 text-xs mt-1">
              <span>{s.ramMinMb / 1024}GB</span>
              <span>{s.ramMaxMb / 1024}GB</span>
            </div>
          </div>
        </section>
      {/if}

      <!-- Java -->
      {#if s.allowJavaPathOverride}
        <section class="mb-8">
          <h2 class="text-white/60 text-xs font-semibold uppercase tracking-wider mb-3">Java</h2>
          <div class="p-4 rounded-xl" style="background: rgba(255,255,255,0.05)">
            <label for="java-path" class="text-white text-sm block mb-2">Ruta de Java (opcional)</label>
            <input
              id="java-path"
              type="text"
              bind:value={s.javaPathOverride}
              placeholder="Detectar automáticamente"
              class="w-full bg-white/10 text-white text-sm px-3 py-2 rounded-lg
                     placeholder:text-white/30 border border-white/10 focus:outline-none
                     focus:border-white/30"
            />
          </div>
        </section>
      {/if}

      <!-- JVM Args -->
      {#if s.allowJvmArgsEdit}
        <section class="mb-8">
          <h2 class="text-white/60 text-xs font-semibold uppercase tracking-wider mb-3">Argumentos JVM adicionales</h2>
          <div class="p-4 rounded-xl" style="background: rgba(255,255,255,0.05)">
            <textarea
              bind:value={s.extraJvmArgs}
              rows="3"
              placeholder="-XX:+UseZGC"
              class="w-full bg-white/10 text-white text-xs px-3 py-2 rounded-lg font-mono
                     placeholder:text-white/30 border border-white/10 focus:outline-none
                     focus:border-white/30 resize-none"
            ></textarea>
          </div>
        </section>
      {/if}

      <!-- Theme -->
      <section class="mb-8">
        <h2 class="text-white/60 text-xs font-semibold uppercase tracking-wider mb-3">Apariencia</h2>
        <div class="p-4 rounded-xl flex gap-3" style="background: rgba(255,255,255,0.05)">
          {#each ['dark', 'light'] as t}
            <button on:click={() => { if(s) s.theme = t; }}
                    class="flex-1 py-2 rounded-lg text-sm font-medium transition-colors"
                    style={s.theme === t ? 'background: var(--color-primary); color: white' : 'color: rgba(255,255,255,0.4)'}>
              {t === 'dark' ? '🌙 Oscuro' : '☀️ Claro'}
            </button>
          {/each}
        </div>
      </section>

      <!-- Actualizaciones del launcher -->
      <section class="mb-8">
        <h2 class="text-white/60 text-xs font-semibold uppercase tracking-wider mb-3">Actualización del launcher</h2>
        <div class="p-4 rounded-xl space-y-3" style="background: rgba(255,255,255,0.05)">

          {#if installDone}
            <!-- Reiniciar -->
            <p class="text-green-400 text-sm font-medium">✅ Actualización instalada. Reinicia el launcher para aplicarla.</p>
          {:else if installing}
            <!-- Progreso de descarga/instalación -->
            <div class="flex items-center gap-2 mb-2">
              <div class="w-4 h-4 border-2 border-white/20 border-t-white/80 rounded-full animate-spin"></div>
              <span class="text-white/70 text-sm">Instalando actualización…</span>
            </div>
            {#if installLogs.length > 0}
              <div class="bg-black/30 rounded-lg p-2 font-mono text-xs text-white/60 max-h-24 overflow-y-auto">
                {#each installLogs as line}
                  <div>{line}</div>
                {/each}
              </div>
            {/if}

          {:else if updateInfo && updateInfo.available}
            <!-- Actualización disponible -->
            <div class="space-y-2">
              <p class="text-white text-sm">
                Nueva versión disponible: <span class="font-bold" style="color: var(--color-accent)">{updateInfo.version}</span>
                <span class="text-white/40 text-xs ml-1">(actual: {updateInfo.currentVersion})</span>
              </p>
              {#if updateInfo.notes}
                <p class="text-white/50 text-xs leading-relaxed">{updateInfo.notes}</p>
              {/if}
              <button on:click={installUpdate}
                      class="px-4 py-1.5 rounded-lg text-sm font-medium text-white transition-all"
                      style="background: var(--color-primary)">
                Instalar actualización
              </button>
            </div>

          {:else}
            <!-- Estado inicial / al día -->
            <div class="flex items-center justify-between">
              <span class="text-white/50 text-sm">
                {updateInfo ? `Al día (${updateInfo.currentVersion})` : 'Comprueba si hay actualizaciones disponibles'}
              </span>
              <button on:click={checkUpdate}
                      disabled={checkingUpdate}
                      class="px-3 py-1.5 rounded-lg text-sm font-medium transition-all
                             disabled:opacity-50 border border-white/20 text-white/80
                             hover:border-white/40 hover:text-white">
                {checkingUpdate ? 'Buscando…' : 'Buscar actualizaciones'}
              </button>
            </div>
          {/if}

        </div>
      </section>

      <!-- Servidor -->
      <section class="mb-8">
        <h2 class="text-white/60 text-xs font-semibold uppercase tracking-wider mb-3">Servidor</h2>
        <div class="p-4 rounded-xl space-y-4" style="background: rgba(255,255,255,0.05)">

          <!-- Forzar sync completo -->
          <div class="flex items-center justify-between gap-4">
            <div class="min-w-0">
              <span class="text-white/80 text-sm font-medium">Forzar sync completo</span>
              <p class="text-white/30 text-xs mt-0.5 leading-snug">
                Marca todos los mods como pendientes. El próximo PLAY los re-verifica
                (los que ya están en caché no se vuelven a descargar).
              </p>
            </div>
            <button on:click={forceSyncReset} disabled={forcingSyncReset}
                    class="shrink-0 px-3 py-1.5 rounded-lg text-sm font-medium transition-all
                           disabled:opacity-50 border border-white/20 text-white/80
                           hover:border-white/40 hover:text-white">
              {forcingSyncReset ? 'Limpiando…' : '⟳ Forzar sync'}
            </button>
          </div>

          <div class="border-t border-white/10"></div>

          <!-- Restablecer options.txt -->
          <div class="flex items-center justify-between gap-4">
            <div class="min-w-0">
              <span class="text-white/60 text-sm">Restablecer opciones del servidor</span>
              <p class="text-white/30 text-xs mt-0.5">Elimina options.txt local; el próximo sync lo restaura a los valores del servidor</p>
            </div>
            <button on:click={resetOptions} disabled={resettingOptions}
                    class="shrink-0 px-3 py-1.5 rounded-lg text-sm font-medium transition-all
                           disabled:opacity-50 border border-white/20 text-white/80
                           hover:border-white/40 hover:text-white">
              {resettingOptions ? 'Eliminando…' : '↺ Restablecer'}
            </button>
          </div>

        </div>
      </section>

      <!-- Diagnóstico -->
      <section class="mb-8">
        <h2 class="text-white/60 text-xs font-semibold uppercase tracking-wider mb-3">Diagnóstico</h2>
        <div class="p-4 rounded-xl space-y-3" style="background: rgba(255,255,255,0.05)">
          <div class="flex items-center justify-between">
            <span class="text-white/60 text-sm">Carpeta de logs</span>
            <button on:click={() => api.logsOpenFolder()}
                    class="text-white/70 hover:text-white text-sm transition-colors">
              📂 Abrir
            </button>
          </div>
          <div class="flex items-center justify-between">
            <div>
              <span class="text-white/60 text-sm">Reporte de soporte</span>
              <p class="text-white/30 text-xs mt-0.5">Genera un archivo con info del sistema y logs</p>
            </div>
            <button on:click={createReport} disabled={creatingReport}
                    class="px-3 py-1.5 rounded-lg text-sm font-medium transition-all
                           disabled:opacity-50 border border-white/20 text-white/80
                           hover:border-white/40 hover:text-white">
              {creatingReport ? 'Creando…' : '📋 Crear'}
            </button>
          </div>
        </div>
      </section>

    {/if}
  </div>
</div>
