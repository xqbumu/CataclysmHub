import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi } from 'vitest';
import BackupsPage from './BackupsPage';
import {
  emptyDashboard,
  type BackupPlan,
  type BackupRecord,
  type BackupTask,
  type Dashboard,
  type HubApi,
} from './api';
const workspace = {
  id: 'workspace',
  save_dir: '/chosen/saves',
  backup_dir: '/chosen/backups',
  content_dir: '/chosen/content',
  created_at: '2026-10-27T00:00:00Z',
};
const plan: BackupPlan = {
  id: 'plan',
  plan_digest: 'digest',
  source_root: workspace.save_dir,
  backup_dir: workspace.backup_dir,
  entry_count: 1,
  file_count: 1,
  total_bytes: '4',
  required_free_bytes: '16777224',
  consistency: 'best_effort',
  expires_at: '2030-01-01T00:00:00Z',
  entries: [
    {
      relative_path: 'unknown.dat',
      file_type: 'file',
      size: '4',
      sha256: null,
    },
  ],
  warnings: ['尽力快照，不保证游戏语义一致。'],
};
const task: BackupTask = {
  id: 'task',
  request_key: 'request',
  workspace_id: workspace.id,
  backup_id: 'backup',
  operation: 'backup.create',
  state: 'running',
  phase: 'copying',
  processed_entries: 1,
  revision: 1,
  error_code: null,
  created_at: workspace.created_at,
  finished_at: null,
};
const record: BackupRecord = {
  id: 'backup',
  task_id: task.id,
  workspace_id: workspace.id,
  relative_dir: 'backup',
  created_at: workspace.created_at,
  entry_count: 1,
  file_count: 1,
  total_bytes: '4',
  consistency: 'best_effort',
  verification: 'verified',
  verified_at: workspace.created_at,
  archive_sha256: 'a'.repeat(64),
  manifest_sha256: 'b'.repeat(64),
  display_name: '',
  notes: '',
  retention_locked: false,
  metadata_revision: 0,
  metadata_updated_at: null,
};
function client(): HubApi {
  return {
    desktop: true,
    dashboard: vi.fn(),
    pickPath: vi.fn(),
    initialize: vi.fn(),
    importInstallation: vi.fn(),
    launchPlan: vi.fn(),
    launch: vi.fn(),
    acknowledge: vi.fn(),
    scanWorlds: vi.fn(),
    cancelScan: vi.fn(),
    scanReport: vi.fn(),
    updateBackupDetails: vi.fn(),
    backupPlan: vi.fn().mockResolvedValue(plan),
    createBackup: vi.fn().mockResolvedValue(task),
    verifyBackup: vi.fn().mockResolvedValue(task),
    restorePlan: vi.fn(),
    startRestore: vi.fn(),
    cancelRestore: vi.fn(),
    startInspection: vi.fn(),
    cancelInspection: vi.fn(),
    retryTaskRecovery: vi.fn(),
    cancelBackup: vi.fn().mockResolvedValue({ ...task, state: 'cancelling' }),
  };
}
function show(api: HubApi, data: Dashboard = { ...emptyDashboard, workspace }) {
  return render(
    <BackupsPage
      api={api}
      data={data}
      busy={false}
      error=""
      run={(action) => action()}
      onConfigure={vi.fn()}
    />,
  );
}
describe('full backup confirmation and integrity display', () => {
  it('requires a configured workspace', () => {
    show(client(), emptyDashboard);
    expect(screen.getByRole('button', { name: '配置工作区' })).toBeEnabled();
    expect(
      screen.queryByRole('button', { name: '预览备份范围' }),
    ).not.toBeInTheDocument();
  });
  it('does not create a backup from preview or cancellation', async () => {
    const api = client();
    const user = userEvent.setup();
    show(api);
    await user.click(screen.getByRole('button', { name: '预览备份范围' }));
    const modal = await screen.findByRole('dialog');
    expect(within(modal).getByText('/chosen/saves')).toBeVisible();
    expect(
      within(modal).getByRole('button', { name: '确认创建完整备份' }),
    ).toBeDisabled();
    await user.click(within(modal).getByRole('button', { name: '取消' }));
    expect(api.createBackup).not.toHaveBeenCalled();
  });
  it('requires explicit best-effort acknowledgement and sends the bound plan digest', async () => {
    const api = client();
    const user = userEvent.setup();
    show(api);
    await user.click(screen.getByRole('button', { name: '预览备份范围' }));
    await screen.findByRole('dialog');
    await user.click(screen.getByRole('checkbox'));
    await user.click(screen.getByRole('button', { name: '确认创建完整备份' }));
    expect(api.createBackup).toHaveBeenCalledExactlyOnceWith(
      plan.id,
      plan.plan_digest,
      expect.stringMatching(/^[a-f0-9-]{36}$/),
    );
  });
  it('keeps integrity and consistency distinct and re-verifies by backup identity', async () => {
    const api = client();
    const user = userEvent.setup();
    show(api, { ...emptyDashboard, workspace, backups: [record] });
    expect(screen.getByText('最近校验通过')).toBeVisible();
    expect(screen.getByText('尽力快照 · 一致性未保证')).toBeVisible();
    expect(screen.getByRole('button', { name: '恢复到新目录' })).toBeEnabled();
    await user.click(screen.getByRole('button', { name: '重新校验' }));
    expect(api.verifyBackup).toHaveBeenCalledExactlyOnceWith(
      record.id,
      expect.any(String),
    );
  });
  it('exposes progress and cancellation without enabling another conflicting write', async () => {
    const api = client();
    const user = userEvent.setup();
    show(api, { ...emptyDashboard, workspace, backup_tasks: [task] });
    expect(screen.getByText('复制冻结副本')).toBeVisible();
    expect(screen.getByRole('button', { name: '预览备份范围' })).toBeDisabled();
    await user.click(screen.getByRole('button', { name: '取消任务' }));
    expect(api.cancelBackup).toHaveBeenCalledExactlyOnceWith(task.id);
  });
  it('preserves interrupted task warnings instead of advertising a successful backup', () => {
    show(client(), {
      ...emptyDashboard,
      workspace,
      backup_tasks: [
        {
          ...task,
          state: 'recovery_required',
          phase: 'interrupted',
          error_code: 'TASK_INTERRUPTED',
        },
      ],
    });
    expect(screen.getByText(/保留的 .partial 临时目录/)).toBeVisible();
    expect(screen.queryByText('最近校验通过')).not.toBeInTheDocument();
  });
  it('shows unavailable archives without claiming current verification passed', () => {
    show(client(), {
      ...emptyDashboard,
      workspace,
      backups: [{ ...record, verification: 'unavailable' }],
    });
    expect(screen.getByText('归档当前不可用')).toBeVisible();
    expect(screen.queryByText('最近校验通过')).not.toBeInTheDocument();
  });
  it('retries only task recording after terminal persistence failure', async () => {
    const api = client();
    show(api, {
      ...emptyDashboard,
      workspace,
      backup_tasks: [
        {
          ...task,
          state: 'recovery_required',
          phase: 'recording_failed',
          error_code: 'TASK_RESULT_PERSIST_FAILED',
        },
      ],
    });
    await userEvent
      .setup()
      .click(screen.getByRole('button', { name: '重试记录任务状态' }));
    expect(api.retryTaskRecovery).toHaveBeenCalledExactlyOnceWith(task.id);
    expect(api.createBackup).not.toHaveBeenCalled();
  });
  it('edits backup annotations only after save and preserves the captured revision', async () => {
    const api = client();
    const user = userEvent.setup();
    show(api, { ...emptyDashboard, workspace, backups: [record] });
    await user.click(screen.getByRole('button', { name: '编辑名称与保留锁' }));
    const modal = await screen.findByRole('dialog');
    await user.type(within(modal).getByLabelText('备份名称'), '升级前');
    await user.type(within(modal).getByLabelText('备注'), '保留此副本');
    await user.click(within(modal).getByRole('checkbox', { name: '保留锁定' }));
    expect(api.updateBackupDetails).not.toHaveBeenCalled();
    await user.click(
      within(modal).getByRole('button', { name: '保存备份信息' }),
    );
    expect(api.updateBackupDetails).toHaveBeenCalledExactlyOnceWith({
      backup_id: record.id,
      display_name: '升级前',
      notes: '保留此副本',
      retention_locked: true,
      expected_revision: 0,
    });
    expect(api.createBackup).not.toHaveBeenCalled();
    expect(api.verifyBackup).not.toHaveBeenCalled();
  });
  it('cancelling annotation edits never saves a name or lock', async () => {
    const api = client();
    const user = userEvent.setup();
    show(api, { ...emptyDashboard, workspace, backups: [record] });
    await user.click(screen.getByRole('button', { name: '编辑名称与保留锁' }));
    const modal = await screen.findByRole('dialog');
    await user.type(within(modal).getByLabelText('备份名称'), 'draft');
    await user.click(within(modal).getByRole('button', { name: '取消' }));
    expect(api.updateBackupDetails).not.toHaveBeenCalled();
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });
  it('shows names and retention markers without hiding creation time or implying filesystem locks', async () => {
    const api = client();
    const user = userEvent.setup();
    show(api, {
      ...emptyDashboard,
      workspace,
      backups: [
        {
          ...record,
          display_name: '长期存档',
          notes: '保留备注',
          retention_locked: true,
        },
      ],
    });
    expect(screen.getByRole('heading', { name: '长期存档' })).toBeVisible();
    expect(screen.getByText(/创建于/)).toBeVisible();
    expect(screen.getByText('命名保留标记')).toBeVisible();
    expect(screen.getByText('保留已锁定')).toBeVisible();
    await user.click(screen.getByRole('button', { name: '编辑名称与保留锁' }));
    expect(
      within(await screen.findByRole('dialog')).getByText(
        /当前尚未开放清理和删除/,
      ),
    ).toBeVisible();
  });
  it('rejects overlong pasted annotation text before submitting', async () => {
    const api = client();
    const user = userEvent.setup();
    show(api, { ...emptyDashboard, workspace, backups: [record] });
    await user.click(screen.getByRole('button', { name: '编辑名称与保留锁' }));
    const modal = await screen.findByRole('dialog');
    await user.click(within(modal).getByLabelText('备份名称'));
    await user.paste('😀'.repeat(129));
    expect(
      within(modal).getByRole('button', { name: '保存备份信息' }),
    ).toBeDisabled();
    expect(within(modal).getByRole('alert')).toHaveTextContent('超过字符上限');
    expect(api.updateBackupDetails).not.toHaveBeenCalled();
  });
  it('keeps unsaved drafts during refresh and reloads the latest revision only on request', async () => {
    const api = client();
    const user = userEvent.setup();
    const rendered = show(api, {
      ...emptyDashboard,
      workspace,
      backups: [record],
    });
    await user.click(screen.getByRole('button', { name: '编辑名称与保留锁' }));
    await screen.findByRole('dialog');
    await user.type(screen.getByLabelText('备份名称'), '旧草稿');
    const updated = {
      ...record,
      display_name: '最新名称',
      notes: '最新备注',
      retention_locked: true,
      metadata_revision: 1,
    };
    vi.mocked(api.dashboard).mockResolvedValue({
      ...emptyDashboard,
      workspace,
      backups: [updated],
    });
    rendered.rerender(
      <BackupsPage
        api={api}
        data={{ ...emptyDashboard, workspace, backups: [updated] }}
        busy={false}
        error=""
        run={(action) => action()}
        onConfigure={vi.fn()}
      />,
    );
    expect(screen.getByLabelText('备份名称')).toHaveValue('旧草稿');
    await user.click(
      screen.getByRole('button', { name: '载入最新信息并替换草稿' }),
    );
    expect(screen.getByLabelText('备份名称')).toHaveValue('最新名称');
    expect(screen.getByLabelText('备注')).toHaveValue('最新备注');
    await user.click(screen.getByRole('button', { name: '保存备份信息' }));
    expect(api.updateBackupDetails).toHaveBeenCalledExactlyOnceWith({
      backup_id: record.id,
      display_name: '最新名称',
      notes: '最新备注',
      retention_locked: true,
      expected_revision: 1,
    });
  });
});
