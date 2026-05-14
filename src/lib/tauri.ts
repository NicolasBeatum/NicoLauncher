import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export type BrandingDto = {
  internalId: string; displayName: string; windowTitle: string;
  primaryColor: string; secondaryColor: string; accentColor: string;
  headingFont: string; bodyFont: string;
  discord: string; website: string;
  serverName: string; serverAddress: string;
};

export type AuthSessionDto = {
  username: string; uuid: string; userType: string;
};

export type ServerManifestDto = {
  manifestVersion: string; mcVersion: string;
  loaderType: string | null; loaderVersion: string | null;
  requiredModsCount: number; optionalModsCount: number;
  announcementId: string | null;
  announcementTitle: string | null; announcementBody: string | null;
  announcementDismissed: boolean;
};

export type ServerStatusDto = {
  online: boolean;
  pingMs: number | null;
  playersOnline: number | null;
  playersMax: number | null;
  motd: string | null;
  version: string | null;
};

export type SyncPlanDto = {
  modsToDownload: number;
  optionalModsToDownload: number;
  modsToRemove: number;
  configsToApply: number; filesToDelete: number;
  loaderAction: string;
};

export type MissingModDto = {
  id: string; name: string; filename: string;
};

/** Server-defined optional mod (from manifest's optional_mods field) */
export type OptionalModDto = {
  id: string; name: string; description: string | null;
  category: string | null; iconUrl: string | null;
  defaultEnabled: boolean; dependsOn: string[]; conflictsWith: string[];
  enabled: boolean;
  inCache: boolean;   // already downloaded → enabling is instant
  linked: boolean;    // currently active in mods/
};

/** A .jar file the user has placed in mods-optional/ */
export type UserModDto = {
  filename: string;
  enabled: boolean;   // true → hardlinked into mods/, loaded by the game
  sizeBytes: number;
};

export type SettingsDto = {
  ramMb: number; ramMinMb: number; ramMaxMb: number;
  javaPathOverride: string | null; extraJvmArgs: string[];
  theme: string; language: string;
  allowRamConfig: boolean; allowJvmArgsEdit: boolean; allowJavaPathOverride: boolean;
};

export type UpdateInfoDto = {
  available: boolean;
  version: string;
  currentVersion: string;
  notes: string | null;
};

export type UpdateStatusDto = {
  logs: string[];
  done: boolean;
  error: string | null;
};

export type InstanceDto = {
  id: string;
  displayName: string;
  description: string;
  color: string;
  icon: string;
  isActive: boolean;
};

export type ProgressEvent = {
  stage?: string; current?: number; total?: number; message?: string;
};

export type ToastEvent = { kind: 'success' | 'error' | 'info'; message: string };

// ── Commands ──────────────────────────────────────────────────────────────────

export const api = {
  getBranding:           ()                       => invoke<BrandingDto>('get_branding'),
  authLogin:             ()                       => invoke<AuthSessionDto>('auth_login_microsoft'),
  authLoginOffline:      (username: string)        => invoke<AuthSessionDto>('auth_login_offline', { username }),
  authLogout:            ()                       => invoke<void>('auth_logout'),
  authCurrentSession:    ()                       => invoke<AuthSessionDto | null>('auth_current_session'),
  authRefresh:           ()                       => invoke<AuthSessionDto>('auth_refresh'),
  manifestFetch:         ()                       => invoke<ServerManifestDto>('manifest_fetch'),
  manifestGetCached:     ()                       => invoke<ServerManifestDto | null>('manifest_get_cached'),
  syncComputePlan:       ()                       => invoke<SyncPlanDto>('sync_compute_plan'),
  syncCheckMissing:      ()                       => invoke<MissingModDto[]>('sync_check_missing'),
  syncApply:             (restoreMods: string[])  => invoke<void>('sync_apply', { restoreMods }),
  syncForceReset:        ()                       => invoke<void>('sync_force_reset'),
  launchGame:            ()                       => invoke<void>('launch_game'),
  getLaunchStatus:       ()                       => invoke<{ logs: string[]; started: boolean; error: string | null; exitCode: number | null }>('get_launch_status'),
  gameIsRunning:         ()                       => invoke<boolean>('game_is_running'),
  gameKill:              ()                       => invoke<void>('game_kill'),
  // Server optional mods (manifest-defined)
  manifestOptionalModsList:     ()                                       => invoke<OptionalModDto[]>('manifest_optional_mods_list'),
  manifestOptionalModSetEnabled:(id: string, enabled: boolean)           => invoke<boolean>('manifest_optional_mod_set_enabled', { id, enabled }),
  syncRebuildOptional:          ()                                       => invoke<string[]>('sync_rebuild_optional'),
  // User-managed optional mods (local .jar files, nothing to do with the server)
  userModsList:          ()                                      => invoke<UserModDto[]>('user_mods_list'),
  userModSetEnabled:     (filename: string, enabled: boolean)    => invoke<void>('user_mod_set_enabled', { filename, enabled }),
  userModsOpenFolder:    ()                                      => invoke<void>('user_mods_open_folder'),
  settingsGet:           ()                       => invoke<SettingsDto>('settings_get'),
  settingsSet:           (settings: SettingsDto)  => invoke<void>('settings_set', { settings }),
  javaDetect:            ()                       => invoke<{ binary: string; majorVersion: number }[]>('java_detect'),
  logsOpenFolder:             ()                       => invoke<void>('logs_open_folder'),
  modsOpenFolder:             ()                       => invoke<void>('mods_open_folder'),
  resetConfigOverride:        (path: string)           => invoke<void>('reset_config_override', { path }),
  createDiagnosticsReport:   ()                       => invoke<string>('create_diagnostics_report'),
  checkUpdate:           ()                       => invoke<UpdateInfoDto | null>('check_update'),
  installUpdate:         ()                       => invoke<void>('install_update'),
  getUpdateStatus:       ()                       => invoke<UpdateStatusDto>('get_update_status'),
  // Manifest extras
  dismissAnnouncement:       (id: string) => invoke<void>('dismiss_announcement', { id }),
  serverStatus:              ()           => invoke<ServerStatusDto>('server_status'),
  // Instances
  getInstances:              ()           => invoke<InstanceDto[]>('get_instances'),
  setActiveInstance:         (id: string) => invoke<void>('set_active_instance', { id }),
  getActiveInstance:         ()           => invoke<string>('get_active_instance'),
  refreshInstancesRegistry:  ()           => invoke<void>('refresh_instances_registry'),
};

// ── Events ────────────────────────────────────────────────────────────────────

export const events = {
  onProgress: (cb: (e: ProgressEvent) => void)     => listen<ProgressEvent>('progress',       e => cb(e.payload)),
  onGameLog:  (cb: (line: string) => void)          => listen<string>('game-log-line',         e => cb(e.payload)),
  onGameExit: (cb: (code: number) => void)          => listen<{code: number}>('game-exited',   e => cb(e.payload.code)),
  onToast:    (cb: (t: ToastEvent) => void)         => listen<ToastEvent>('toast',             e => cb(e.payload)),
  onAppLog:   (cb: (line: string) => void)          => listen<string>('app-log',               e => cb(e.payload)),
  onGameStarted: (cb: () => void)                   => listen('game-started',                   () => cb()),
  onLaunchError: (cb: (msg: string) => void)        => listen<string>('launch-error',           e => cb(e.payload)),
  onManifestUpdated: (cb: () => void)               => listen('manifest-updated',               () => cb()),
};
