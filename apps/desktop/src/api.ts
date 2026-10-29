import type { InstallationRecognition } from './bindings/InstallationRecognition';
import type { HubPreferences } from './bindings/HubPreferences';
import type { PreferencesReset } from './bindings/PreferencesReset';
import type { ConfigChangePlan } from './bindings/ConfigChangePlan';
import type { ConfigChange } from './bindings/ConfigChange';
import type { ConfigChangeObservation } from './bindings/ConfigChangeObservation';
import type { ContentConfigProtectionPlan } from './bindings/ContentConfigProtectionPlan';
import type { ContentConfigInput } from './bindings/ContentConfigInput';
import type { ContentConfigPlan } from './bindings/ContentConfigPlan';
import type { ContentConfigReport } from './bindings/ContentConfigReport';
import type { ContentInstallPlan } from './bindings/ContentInstallPlan';
import type { ContentInstallTask } from './bindings/ContentInstallTask';
import type { ContentEntry } from './bindings/ContentEntry';
import type { ManagedInstallationCopyPlan } from './bindings/ManagedInstallationCopyPlan';
import type { InstallationCopyTask } from './bindings/InstallationCopyTask';
import type { InstanceResourcePreview } from './bindings/InstanceResourcePreview';
import type { PlayInstance } from './bindings/PlayInstance';
import type { PlayInstanceInput } from './bindings/PlayInstanceInput';
import { invoke, isTauri } from '@tauri-apps/api/core';

import type { Workspace } from './bindings/Workspace';
import type { WorkspaceInput } from './bindings/WorkspaceInput';
import type { Installation } from './bindings/Installation';
import type { Session } from './bindings/Session';
import type { OperationLog } from './bindings/OperationLog';
import type { Dashboard } from './bindings/Dashboard';
import type { LaunchPlan } from './bindings/LaunchPlan';
import type { ScanTask } from './bindings/ScanTask';
import type { ScanReport } from './bindings/ScanReport';
import type { BackupPlan } from './bindings/BackupPlan';
import type { BackupTask } from './bindings/BackupTask';
import type { InspectionTask } from './bindings/InspectionTask';
import type { RestorePlan } from './bindings/RestorePlan';
import type { RestoreTask } from './bindings/RestoreTask';
import type { RetentionPolicy } from './bindings/RetentionPolicy';
import type { RetentionPreview } from './bindings/RetentionPreview';
import type { BackupDetailsInput } from './bindings/BackupDetailsInput';
import type { BackupRecord } from './bindings/BackupRecord';
export type {
  Workspace,
  WorkspaceInput,
  Installation,
  Session,
  OperationLog,
  Dashboard,
  LaunchPlan,
  ScanTask,
  ScanReport,
  BackupPlan,
  BackupTask,
  BackupRecord,
  BackupDetailsInput,
  RetentionPolicy,
  RetentionPreview,
  RestorePlan,
  RestoreTask,
  InspectionTask,
};
export const emptyDashboard: Dashboard = {
  copy_tasks: [],
  resource_claims: [],
  workspace: null,
  installations: [],
  play_instances: [],
  sessions: [],
  operations: [],
  scan_tasks: [],
  backup_tasks: [],
  backups: [],
  restore_tasks: [],
  inspection_tasks: [],
};
export interface HubApi {
  inspectInstallationRecognition?(id: string): Promise<InstallationRecognition>;
  registerManagedInstance(
    installationId: string,
    name: string,
  ): Promise<PlayInstance>;
  workspaceDefaults(): Promise<WorkspaceInput | null>;
  preferences(): Promise<HubPreferences>;
  saveInstanceProfile(
    instanceId: string,
    profile: string,
  ): Promise<HubPreferences>;
  resetPreferences(
    revision: number,
    confirmed: boolean,
  ): Promise<PreferencesReset>;

  prepareConfigRollback(changeId: string): Promise<ConfigChangePlan>;
  prepareConfigChange(
    instanceId: string,
    input: ContentConfigInput,
    backupId: string,
  ): Promise<ConfigChangePlan>;
  applyConfigChange(
    planId: string,
    digest: string,
    requestKey: string,
  ): Promise<ConfigChange>;
  configChanges(): Promise<ConfigChange[]>;
  inspectConfigChange(id: string): Promise<ConfigChangeObservation>;
  resolveConfigChange(
    id: string,
    expectedHash: string,
  ): Promise<ConfigChangeObservation>;
  prepareContentConfigProtection(
    instanceId: string,
    input: ContentConfigInput,
  ): Promise<ContentConfigProtectionPlan>;
  previewContentConfig(
    instanceId: string,
    input: ContentConfigInput,
  ): Promise<ContentConfigPlan>;
  inspectContentConfig(instanceId: string): Promise<ContentConfigReport>;
  prepareContentInstall(
    entryId: string,
    parentDir: string,
  ): Promise<ContentInstallPlan>;
  startContentInstall(
    planId: string,
    digest: string,
    key: string,
    confirmed: boolean,
  ): Promise<string>;
  contentInstallTasks(): Promise<ContentInstallTask[]>;
  cancelContentInstall(id: string): Promise<void>;
  contentEntries(): Promise<ContentEntry[]>;
  registerContent(root: string, kind: string): Promise<ContentEntry>;
  refreshContent(id: string): Promise<ContentEntry>;
  forgetContent(id: string): Promise<void>;
  prepareInstallationCopy(
    installationId: string,
    sourceRoot: string,
    parentDir: string,
  ): Promise<ManagedInstallationCopyPlan>;
  startInstallationCopy(
    planId: string,
    planDigest: string,
    requestKey: string,
    confirmed: boolean,
  ): Promise<InstallationCopyTask>;
  cancelInstallationCopy(taskId: string): Promise<InstallationCopyTask>;
  registerCopiedInstallation(
    taskId: string,
    name: string,
  ): Promise<Installation>;

  previewInstanceResources(
    instanceId: string,
  ): Promise<InstanceResourcePreview>;
  registerPlayInstance(input: PlayInstanceInput): Promise<PlayInstance>;
  desktop: boolean;
  dashboard(): Promise<Dashboard>;
  pickPath(kind: 'directory' | 'executable'): Promise<string | null>;
  initialize(input: WorkspaceInput): Promise<Workspace>;
  importInstallation(executable: string, name: string): Promise<Installation>;
  instanceLaunchPlan(
    instanceId: string,
    profileId: string,
  ): Promise<LaunchPlan>;
  launchPlan(installationId: string, args: string[]): Promise<LaunchPlan>;
  launch(planId: string): Promise<Session>;
  acknowledge(sessionId: string): Promise<void>;
  scanWorlds(requestKey: string): Promise<ScanTask>;
  cancelScan(taskId: string): Promise<ScanTask>;
  scanReport(taskId: string): Promise<ScanReport>;
  previewRetention(policy: RetentionPolicy): Promise<RetentionPreview>;
  updateBackupDetails(input: BackupDetailsInput): Promise<BackupRecord>;
  backupPlan(): Promise<BackupPlan>;
  instanceBackupPlan(instanceId: string): Promise<BackupPlan>;
  createBackup(
    planId: string,
    planDigest: string,
    requestKey: string,
  ): Promise<BackupTask>;
  verifyBackup(backupId: string, requestKey: string): Promise<BackupTask>;
  restorePlan(backupId: string, parentDir: string): Promise<RestorePlan>;
  startRestore(
    planId: string,
    planDigest: string,
    requestKey: string,
  ): Promise<RestoreTask>;
  startInspection(
    restoreTaskId: string,
    requestKey: string,
  ): Promise<InspectionTask>;
  cancelInspection(taskId: string): Promise<InspectionTask>;
  cancelRestore(taskId: string): Promise<RestoreTask>;
  retryTaskRecovery(taskId: string): Promise<void>;
  cancelBackup(taskId: string): Promise<BackupTask>;
}
function desktopOnly<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (!isTauri())
    return Promise.reject({
      code: 'DESKTOP_REQUIRED',
      message:
        '请使用 npm run desktop dev 启动桌面应用；浏览器预览无法访问本地文件。',
    });
  return invoke<T>(command, args);
}
export const api: HubApi = {
  inspectInstallationRecognition: (id) =>
    desktopOnly('inspect_installation_recognition', { id }),
  registerManagedInstance: (installationId, name) =>
    desktopOnly('register_managed_instance', { installationId, name }),
  workspaceDefaults: () =>
    isTauri() ? desktopOnly('workspace_defaults') : Promise.resolve(null),
  preferences: () =>
    isTauri()
      ? desktopOnly('hub_preferences')
      : Promise.resolve({ revision: 0, instance_profiles: {} }),
  saveInstanceProfile: (instanceId, profile) =>
    desktopOnly('save_instance_profile', { instanceId, profile }),
  resetPreferences: (revision, confirmed) =>
    desktopOnly('reset_hub_preferences', { revision, confirmed }),

  prepareConfigRollback: (changeId) =>
    desktopOnly('prepare_config_rollback', { changeId }),
  prepareConfigChange: (instanceId, input, backupId) =>
    desktopOnly('prepare_config_change', { instanceId, input, backupId }),
  applyConfigChange: (planId, digest, requestKey) =>
    desktopOnly('apply_config_change', {
      planId,
      digest,
      requestKey,
      confirmed: true,
    }),
  configChanges: () => desktopOnly('config_changes', {}),
  inspectConfigChange: (id) => desktopOnly('inspect_config_change', { id }),
  resolveConfigChange: (id, expectedHash) =>
    desktopOnly('resolve_config_change', { id, expectedHash, confirmed: true }),
  prepareContentConfigProtection: (instanceId, input) =>
    desktopOnly('prepare_content_config_protection', { instanceId, input }),
  previewContentConfig: (instanceId, input) =>
    desktopOnly('preview_content_config', { instanceId, input }),
  inspectContentConfig: (instanceId) =>
    desktopOnly('inspect_content_config', { instanceId }),
  prepareContentInstall: (entryId, parentDir) =>
    desktopOnly('prepare_content_install', { entryId, parentDir }),
  startContentInstall: (planId, digest, key, confirmed) =>
    desktopOnly('start_content_install', { planId, digest, key, confirmed }),
  contentInstallTasks: () => desktopOnly('content_install_tasks'),
  cancelContentInstall: (id) => desktopOnly('cancel_content_install', { id }),
  contentEntries: () => desktopOnly('content_entries'),
  registerContent: (root, kind) =>
    desktopOnly('register_content', { root, kind }),
  refreshContent: (id) => desktopOnly('refresh_content', { id }),
  forgetContent: (id) => desktopOnly('forget_content', { id }),
  prepareInstallationCopy: (installationId, sourceRoot, parentDir) =>
    desktopOnly('prepare_installation_copy', {
      installationId,
      sourceRoot,
      parentDir,
    }),
  startInstallationCopy: (planId, planDigest, requestKey, confirmed) =>
    desktopOnly('start_installation_copy', {
      planId,
      planDigest,
      requestKey,
      confirmed,
    }),
  cancelInstallationCopy: (taskId) =>
    desktopOnly('cancel_installation_copy', { taskId }),
  registerCopiedInstallation: (taskId, name) =>
    desktopOnly('register_copied_installation', { taskId, name }),

  previewInstanceResources: (instanceId) =>
    desktopOnly('preview_instance_resources', { instanceId }),
  registerPlayInstance: (input) =>
    desktopOnly('register_play_instance', { input }),
  desktop: isTauri(),
  dashboard: () =>
    isTauri() ? desktopOnly('get_dashboard') : Promise.resolve(emptyDashboard),
  pickPath: (kind) => desktopOnly('pick_path', { kind }),
  initialize: (input) => desktopOnly('initialize_workspace', { input }),
  importInstallation: (executable, name) =>
    desktopOnly('import_installation', { executable, name }),
  instanceLaunchPlan: (instanceId, profileId) =>
    desktopOnly('create_instance_launch_plan', { instanceId, profileId }),
  launchPlan: (installationId, args) =>
    desktopOnly('create_launch_plan', { installationId, args }),
  launch: (planId) => desktopOnly('launch_game', { planId, confirmed: true }),
  acknowledge: (sessionId) =>
    desktopOnly('acknowledge_interrupted', { sessionId }),
  scanWorlds: (requestKey) => desktopOnly('start_world_scan', { requestKey }),
  cancelScan: (taskId) => desktopOnly('cancel_world_scan', { taskId }),
  scanReport: (taskId) => desktopOnly('get_scan_report', { taskId }),
  previewRetention: (policy) => desktopOnly('preview_retention', { policy }),
  updateBackupDetails: (input) =>
    desktopOnly('update_backup_details', { input }),
  backupPlan: () => desktopOnly('prepare_backup'),
  instanceBackupPlan: (instanceId) =>
    desktopOnly('prepare_instance_backup', { instanceId }),
  createBackup: (planId, planDigest, requestKey) =>
    desktopOnly('create_backup', {
      planId,
      planDigest,
      requestKey,
      confirmed: true,
    }),
  verifyBackup: (backupId, requestKey) =>
    desktopOnly('verify_backup', { backupId, requestKey }),
  restorePlan: (backupId, parentDir) =>
    desktopOnly('prepare_restore', { backupId, parentDir }),
  startRestore: (planId, planDigest, requestKey) =>
    desktopOnly('start_restore', {
      planId,
      planDigest,
      requestKey,
      confirmed: true,
    }),
  cancelRestore: (taskId) => desktopOnly('cancel_restore', { taskId }),
  startInspection: (restoreTaskId, requestKey) =>
    desktopOnly('start_inspection', { restoreTaskId, requestKey }),
  cancelInspection: (taskId) => desktopOnly('cancel_inspection', { taskId }),
  retryTaskRecovery: (taskId) => desktopOnly('retry_task_recovery', { taskId }),
  cancelBackup: (taskId) => desktopOnly('cancel_backup', { taskId }),
};
export function errorMessage(error: unknown): string {
  if (
    typeof error === 'object' &&
    error !== null &&
    'message' in error &&
    typeof error.message === 'string'
  )
    return error.message;
  return '操作未完成，请重试并检查本地权限。';
}
export function parseArguments(value: string): string[] {
  const parsed: unknown = JSON.parse(value);
  if (
    !Array.isArray(parsed) ||
    !parsed.every((item): item is string => typeof item === 'string') ||
    parsed.length > 64 ||
    parsed.some((item) => item.includes('\0')) ||
    new TextEncoder().encode(parsed.join('')).length > 8192
  ) {
    throw new Error(
      '请填写 JSON 字符串数组，例如 ["--example", "value"]，最多 64 个参数。',
    );
  }
  return parsed;
}
