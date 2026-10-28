import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import InspectionPanel from './InspectionPanel';
import { RestoreButton, RestoreTasks } from './RestoreWorkflow';
import type { HubApi, RestorePlan, RestoreTask, InspectionTask } from './api';
const plan: RestorePlan = {
  id: 'plan',
  plan_digest: 'digest',
  backup_id: 'backup',
  destination: '/chosen/restored-id',
  parent_dir: '/chosen',
  entry_count: 4,
  file_count: 2,
  total_bytes: '123',
  consistency: 'best_effort',
  archive_sha256: 'a'.repeat(64),
  manifest_sha256: 'b'.repeat(64),
  expires_at: '2030-01-01T00:00:00Z',
  warnings: ['不自动切换当前存档。'],
};
const task: RestoreTask = {
  id: 'restore-task',
  request_key: 'key',
  workspace_id: 'workspace',
  backup_id: plan.backup_id,
  destination: plan.destination,
  state: 'running',
  phase: 'checking_restored',
  revision: 1,
  error_code: null,
  created_at: '2026-10-27T00:00:00Z',
  finished_at: null,
};
function client() {
  return {
    desktop: true,
    prepareContentInstall: vi.fn(),
    startContentInstall: vi.fn(),
    contentInstallTasks: vi.fn().mockResolvedValue([]),
    cancelContentInstall: vi.fn(),
    contentEntries: vi.fn().mockResolvedValue([]),
    registerContent: vi.fn(),
    refreshContent: vi.fn(),
    forgetContent: vi.fn(),
    prepareInstallationCopy: vi.fn(),
    startInstallationCopy: vi.fn(),
    cancelInstallationCopy: vi.fn(),
    registerCopiedInstallation: vi.fn(),
    dashboard: vi.fn(),
    pickPath: vi.fn().mockResolvedValue('/chosen'),
    initialize: vi.fn(),
    importInstallation: vi.fn(),
    registerPlayInstance: vi.fn(),
    instanceLaunchPlan: vi.fn(),
    previewInstanceResources: vi.fn(),
    launchPlan: vi.fn(),
    launch: vi.fn(),
    acknowledge: vi.fn(),
    scanWorlds: vi.fn(),
    cancelScan: vi.fn(),
    scanReport: vi.fn(),
    previewRetention: vi.fn(),
    updateBackupDetails: vi.fn(),
    backupPlan: vi.fn(),
    instanceBackupPlan: vi.fn(),
    createBackup: vi.fn(),
    verifyBackup: vi.fn(),
    cancelBackup: vi.fn(),
    startInspection: vi.fn(),
    cancelInspection: vi.fn(),
    retryTaskRecovery: vi.fn(),
    restorePlan: vi.fn().mockResolvedValue(plan),
    startRestore: vi.fn().mockResolvedValue(task),
    cancelRestore: vi.fn().mockResolvedValue({ ...task, state: 'cancelling' }),
  } satisfies HubApi;
}
function show(api: HubApi, blocked = false) {
  return render(
    <RestoreButton
      api={api}
      backupId="backup"
      busy={false}
      blocked={blocked}
      error=""
      run={(action) => action()}
    />,
  );
}
describe('new-directory restore workflow', () => {
  it('does nothing after the native picker is cancelled', async () => {
    const api = client();
    api.pickPath.mockResolvedValue(null);
    show(api);
    await userEvent
      .setup()
      .click(screen.getByRole('button', { name: '恢复到新目录' }));
    expect(api.pickPath).toHaveBeenCalledExactlyOnceWith('directory');
    expect(api.restorePlan).not.toHaveBeenCalled();
    expect(api.startRestore).not.toHaveBeenCalled();
  });
  it('previews the exact new directory and requires acknowledgement before sending a bound plan', async () => {
    const api = client();
    const user = userEvent.setup();
    show(api);
    await user.click(screen.getByRole('button', { name: '恢复到新目录' }));
    const modal = await screen.findByRole('dialog');
    expect(api.restorePlan).toHaveBeenCalledExactlyOnceWith(
      'backup',
      '/chosen',
    );
    expect(within(modal).getByText(plan.destination)).toBeVisible();
    expect(
      within(modal).getByRole('button', { name: '确认恢复到新目录' }),
    ).toBeDisabled();
    expect(api.startRestore).not.toHaveBeenCalled();
    await user.click(within(modal).getByRole('checkbox'));
    await user.click(
      within(modal).getByRole('button', { name: '确认恢复到新目录' }),
    );
    expect(api.startRestore).toHaveBeenCalledExactlyOnceWith(
      plan.id,
      plan.plan_digest,
      expect.stringMatching(/^[a-f0-9-]{36}$/),
    );
  });
  it('closing the confirmation dialog never starts recovery', async () => {
    const api = client();
    const user = userEvent.setup();
    show(api);
    await user.click(screen.getByRole('button', { name: '恢复到新目录' }));
    const modal = await screen.findByRole('dialog');
    await user.click(within(modal).getByRole('button', { name: '取消' }));
    expect(api.startRestore).not.toHaveBeenCalled();
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });
  it('prevents new recovery while the workspace is busy', () => {
    show(client(), true);
    expect(screen.getByRole('button', { name: '恢复到新目录' })).toBeDisabled();
  });
  it('shows readback progress and sends cancellation by persisted task identity', async () => {
    const api = client();
    render(
      <RestoreTasks
        api={api}
        tasks={[task]}
        busy={false}
        run={(action) => action()}
      />,
    );
    expect(screen.getByText('回读验证恢复文件')).toBeVisible();
    await userEvent
      .setup()
      .click(screen.getByRole('button', { name: '取消恢复' }));
    expect(api.cancelRestore).toHaveBeenCalledExactlyOnceWith(task.id);
  });
  it('keeps interrupted output visible and only retries recording after persistence failure', async () => {
    const api = client();
    render(
      <RestoreTasks
        api={api}
        tasks={[
          {
            ...task,
            state: 'recovery_required',
            error_code: 'TASK_RESULT_PERSIST_FAILED',
          },
        ]}
        busy={false}
        run={(action) => action()}
      />,
    );
    expect(screen.getByText(plan.destination)).toBeVisible();
    expect(screen.getByText(/不会自动清理或重新执行/)).toBeVisible();
    await userEvent
      .setup()
      .click(screen.getByRole('button', { name: '重试记录任务状态' }));
    expect(api.retryTaskRecovery).toHaveBeenCalledExactlyOnceWith(task.id);
    expect(api.startRestore).not.toHaveBeenCalled();
  });
  it('does not present successful byte verification as game compatibility', () => {
    render(
      <RestoreTasks
        api={client()}
        tasks={[{ ...task, state: 'succeeded', phase: 'finished' }]}
        busy={false}
        run={(action) => action()}
      />,
    );
    expect(screen.getByText('恢复完成 · 当时字节已验证')).toBeVisible();
    expect(screen.getByText(/游戏兼容性仍未验证/)).toBeVisible();
    expect(
      screen.queryByRole('button', { name: '取消恢复' }),
    ).not.toBeInTheDocument();
  });
});

const inspection: InspectionTask = {
  id: 'inspection',
  request_key: 'inspection-key',
  restore_task_id: task.id,
  state: 'succeeded',
  phase: 'finished',
  revision: 2,
  error_code: null,
  created_at: '2026-10-27T01:00:00Z',
  finished_at: '2026-10-27T01:01:00Z',
  result: {
    schema_version: 1,
    inspected_at: '2026-10-27T01:01:00Z',
    incomplete_marker_present: true,
    historical_report_present: true,
    verification: {
      schema_version: 1,
      state: 'verified',
      restored_files: 2,
      restored_entries: 4,
      restored_bytes: '123',
      backup: {
        backup_id: 'backup',
        file_count: 2,
        entry_count: 4,
        total_bytes: '123',
        consistency: 'best_effort',
        archive_sha256: 'a'.repeat(64),
        manifest_sha256: 'b'.repeat(64),
        verified_at: '2026-10-27T01:00:00Z',
        receipt_present: true,
      },
    },
  },
};
function inspectPanel(
  api: HubApi,
  inspections: InspectionTask[] = [],
  blocked = false,
) {
  return render(
    <InspectionPanel
      api={api}
      restoreTaskId={task.id}
      inspections={inspections}
      busy={false}
      blocked={blocked}
      run={(action) => action()}
    />,
  );
}
describe('persistent recovery directory inspection', () => {
  it('starts read-only inspection by original restore identity without selecting arbitrary paths', async () => {
    const api = client();
    inspectPanel(api);
    await userEvent
      .setup()
      .click(screen.getByRole('button', { name: '检查已恢复目录' }));
    expect(api.startInspection).toHaveBeenCalledExactlyOnceWith(
      task.id,
      expect.stringMatching(/^[a-f0-9-]{36}$/),
    );
    expect(api.pickPath).not.toHaveBeenCalled();
    expect(api.startRestore).not.toHaveBeenCalled();
  });
  it('keeps cancellation available while other conflicting buttons remain blocked', async () => {
    const api = client();
    inspectPanel(
      api,
      [
        {
          ...inspection,
          state: 'running',
          phase: 'checking_restored',
          result: null,
        },
      ],
      true,
    );
    expect(
      screen.getByRole('button', { name: '检查已恢复目录' }),
    ).toBeDisabled();
    expect(screen.getByText('正在检查目录')).toBeVisible();
    await userEvent
      .setup()
      .click(screen.getByRole('button', { name: '取消检查' }));
    expect(api.cancelInspection).toHaveBeenCalledExactlyOnceWith(inspection.id);
  });
  it('shows matching bytes and incomplete workflow as separate facts', () => {
    render(
      <RestoreTasks
        api={client()}
        tasks={[
          {
            ...task,
            state: 'recovery_required',
            phase: 'interrupted',
            error_code: 'TASK_INTERRUPTED',
          },
        ]}
        inspections={[inspection]}
        busy={false}
        run={(action) => action()}
      />,
    );
    expect(screen.getByText('需要检查目标目录')).toBeVisible();
    expect(screen.getByText('本次文件检查通过')).toBeVisible();
    expect(
      screen.getByText(/仍有未完成标记；字节匹配不代表原恢复流程已完成/),
    ).toBeVisible();
    expect(
      screen.queryByText('恢复完成 · 当时字节已验证'),
    ).not.toBeInTheDocument();
  });
  it('does not show an older successful verdict after the latest inspection failed', () => {
    const failed = {
      ...inspection,
      id: 'failed',
      state: 'failed',
      error_code: 'RESTORED_DATA_MISMATCH',
      result: null,
    };
    inspectPanel(client(), [failed, inspection]);
    expect(screen.getByText('本次检查未通过')).toBeVisible();
    expect(screen.getByText('RESTORED_DATA_MISMATCH')).toBeVisible();
    expect(screen.queryByText('本次文件检查通过')).not.toBeInTheDocument();
  });
  it('retries only status recording after terminal persistence failure', async () => {
    const api = client();
    inspectPanel(api, [
      {
        ...inspection,
        state: 'recovery_required',
        error_code: 'TASK_RESULT_PERSIST_FAILED',
        result: null,
      },
    ]);
    await userEvent
      .setup()
      .click(screen.getByRole('button', { name: '重试记录检查状态' }));
    expect(api.retryTaskRecovery).toHaveBeenCalledExactlyOnceWith(
      inspection.id,
    );
    expect(api.startInspection).not.toHaveBeenCalled();
  });
  it('reports cancellation without presenting a fresh verification verdict', () => {
    inspectPanel(client(), [
      { ...inspection, state: 'cancelled', result: null },
    ]);
    expect(screen.getByText('检查已取消，未形成新结论')).toBeVisible();
    expect(screen.queryByText('本次文件检查通过')).not.toBeInTheDocument();
  });
});
