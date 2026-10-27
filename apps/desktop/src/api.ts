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
import type { RestorePlan } from './bindings/RestorePlan';
import type { RestoreTask } from './bindings/RestoreTask';
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
  RestorePlan,
  RestoreTask,
};
export const emptyDashboard: Dashboard = {
  workspace: null,
  installations: [],
  sessions: [],
  operations: [],
  scan_tasks: [],
  backup_tasks: [],
  backups: [],
  restore_tasks: [],
};
export interface HubApi {
  desktop: boolean;
  dashboard(): Promise<Dashboard>;
  pickPath(kind: 'directory' | 'executable'): Promise<string | null>;
  initialize(input: WorkspaceInput): Promise<Workspace>;
  importInstallation(executable: string, name: string): Promise<Installation>;
  launchPlan(installationId: string, args: string[]): Promise<LaunchPlan>;
  launch(planId: string): Promise<Session>;
  acknowledge(sessionId: string): Promise<void>;
  scanWorlds(requestKey: string): Promise<ScanTask>;
  cancelScan(taskId: string): Promise<ScanTask>;
  scanReport(taskId: string): Promise<ScanReport>;
  backupPlan(): Promise<BackupPlan>;
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
  desktop: isTauri(),
  dashboard: () =>
    isTauri() ? desktopOnly('get_dashboard') : Promise.resolve(emptyDashboard),
  pickPath: (kind) => desktopOnly('pick_path', { kind }),
  initialize: (input) => desktopOnly('initialize_workspace', { input }),
  importInstallation: (executable, name) =>
    desktopOnly('import_installation', { executable, name }),
  launchPlan: (installationId, args) =>
    desktopOnly('create_launch_plan', { installationId, args }),
  launch: (planId) => desktopOnly('launch_game', { planId, confirmed: true }),
  acknowledge: (sessionId) =>
    desktopOnly('acknowledge_interrupted', { sessionId }),
  scanWorlds: (requestKey) => desktopOnly('start_world_scan', { requestKey }),
  cancelScan: (taskId) => desktopOnly('cancel_world_scan', { taskId }),
  scanReport: (taskId) => desktopOnly('get_scan_report', { taskId }),
  backupPlan: () => desktopOnly('prepare_backup'),
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
