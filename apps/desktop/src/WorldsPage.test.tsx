import { render, screen, within, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi } from 'vitest';
import WorldsPage from './WorldsPage';
import {
  emptyDashboard,
  type HubApi,
  type ScanReport,
  type ScanTask,
  type Dashboard,
} from './api';
const workspace = {
  id: 'workspace',
  save_dir: '/chosen/saves',
  backup_dir: '/chosen/backups',
  content_dir: '/chosen/content',
  created_at: '2026-10-27T00:00:00Z',
};
const task: ScanTask = {
  id: 'scan',
  request_key: 'key',
  workspace_id: workspace.id,
  operation: 'world.scan',
  state: 'succeeded',
  phase: 'finished',
  processed_entries: 3,
  revision: 2,
  error_code: null,
  created_at: workspace.created_at,
  finished_at: workspace.created_at,
};
const report: ScanReport = {
  task_id: task.id,
  scanner_version: 'opaque-directory-v1',
  created_at: workspace.created_at,
  enumeration_complete: true,
  visited_entries: 3,
  total_bytes: '4',
  candidates: [
    {
      id: 'candidate',
      relative_root: 'UnverifiedWorld',
      file_count: 1,
      entry_count: 2,
      total_bytes: '4',
      identity_confidence: 'unknown',
      boundary_verified: false,
    },
  ],
  entries: [
    {
      relative_path: 'UnverifiedWorld',
      file_type: 'directory',
      size: '0',
      candidate_root: 'UnverifiedWorld',
    },
    {
      relative_path: 'UnverifiedWorld/same-name.sav',
      file_type: 'file',
      size: '4',
      candidate_root: 'UnverifiedWorld',
    },
    {
      relative_path: 'shared',
      file_type: 'file',
      size: '0',
      candidate_root: null,
    },
  ],
  issues: [],
};
function client(): HubApi {
  return {
    desktop: true,
    dashboard: vi.fn(),
    pickPath: vi.fn(),
    initialize: vi.fn(),
    importInstallation: vi.fn(),
    registerPlayInstance: vi.fn(),
    instanceLaunchPlan: vi.fn(),
    launchPlan: vi.fn(),
    launch: vi.fn(),
    acknowledge: vi.fn(),
    scanWorlds: vi.fn().mockResolvedValue(task),
    cancelScan: vi.fn().mockResolvedValue({ ...task, state: 'cancelling' }),
    previewRetention: vi.fn(),
    updateBackupDetails: vi.fn(),
    backupPlan: vi.fn(),
    createBackup: vi.fn(),
    verifyBackup: vi.fn(),
    restorePlan: vi.fn(),
    startRestore: vi.fn(),
    cancelRestore: vi.fn(),
    startInspection: vi.fn(),
    cancelInspection: vi.fn(),
    retryTaskRecovery: vi.fn(),
    cancelBackup: vi.fn(),
    scanReport: vi.fn().mockResolvedValue(report),
  };
}
function show(
  api: HubApi,
  data: Dashboard = { ...emptyDashboard, workspace, scan_tasks: [task] },
) {
  return render(
    <WorldsPage
      api={api}
      data={data}
      busy={false}
      run={(action) => action()}
      onConfigure={vi.fn()}
    />,
  );
}
describe('read-only inventory', () => {
  it('requires a configured root before scanning', () => {
    show(client(), emptyDashboard);
    expect(screen.getByRole('button', { name: '配置工作区' })).toBeEnabled();
    expect(
      screen.queryByRole('button', { name: '只读扫描' }),
    ).not.toBeInTheDocument();
  });
  it('keeps candidates unverified and separates shared files without merging character names', async () => {
    const user = userEvent.setup();
    show(client());
    await screen.findByText('世界身份未确认 · 角色未知');
    await user.click(screen.getByRole('button', { name: /共享与未归属文件/ }));
    const table = screen.getByRole('table');
    expect(within(table).getByText('shared')).toBeVisible();
    expect(
      within(table).queryByText('UnverifiedWorld/same-name.sav'),
    ).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: /UnverifiedWorld/ }));
    expect(await screen.findByText('边界未验证')).toBeVisible();
    expect(
      within(table).getByText('UnverifiedWorld/same-name.sav'),
    ).toBeVisible();
  });
  it('makes an incomplete report and skipped links explicit', async () => {
    const api = client();
    vi.mocked(api.scanReport).mockResolvedValue({
      ...report,
      enumeration_complete: false,
      issues: [
        { code: 'LINK_NOT_FOLLOWED', relative_path: 'linked-world' },
        { code: 'ENTRY_LIMIT', relative_path: '' },
      ],
    });
    show(api);
    expect(await screen.findByRole('alert')).toHaveTextContent(
      '扫描清单不完整',
    );
    expect(screen.getByText('链接未跟随，目标内容未扫描')).toBeVisible();
    expect(screen.getByText('达到 20,000 个条目的扫描上限')).toBeVisible();
  });
  it('starts an idempotent scan without invoking launch or other file operations', async () => {
    const api = client();
    const user = userEvent.setup();
    show(api, { ...emptyDashboard, workspace });
    await user.click(screen.getByRole('button', { name: '只读扫描' }));
    expect(api.scanWorlds).toHaveBeenCalledTimes(1);
    expect(vi.mocked(api.scanWorlds).mock.calls[0][0]).toMatch(
      /^[0-9a-f-]{36}$/,
    );
    expect(api.launch).not.toHaveBeenCalled();
    expect(api.initialize).not.toHaveBeenCalled();
  });
  it('offers cancellation while active and displays persisted progress', async () => {
    const api = client();
    const user = userEvent.setup();
    show(api, {
      ...emptyDashboard,
      workspace,
      scan_tasks: [{ ...task, state: 'running', processed_entries: 42 }],
    });
    expect(screen.getByText('已枚举 42 个条目')).toBeVisible();
    expect(screen.getByRole('button', { name: '只读扫描' })).toBeDisabled();
    await user.click(screen.getByRole('button', { name: '取消扫描' }));
    expect(api.cancelScan).toHaveBeenCalledExactlyOnceWith(task.id);
  });
  it('paginates large inventories and filters literal path text', async () => {
    const api = client();
    const user = userEvent.setup();
    vi.mocked(api.scanReport).mockResolvedValue({
      ...report,
      entries: Array.from({ length: 61 }, (_, i) => ({
        relative_path: `file-${i}`,
        file_type: 'file',
        size: '0',
        candidate_root: null,
      })),
    });
    show(api);
    await screen.findByText('61 个匹配条目 · 第 1 页');
    expect(within(screen.getByRole('table')).getAllByRole('row')).toHaveLength(
      51,
    );
    await user.click(screen.getByRole('button', { name: '下一页' }));
    expect(screen.getByText('61 个匹配条目 · 第 2 页')).toBeVisible();
    await user.type(screen.getByLabelText('筛选相对路径'), 'file-60');
    expect(screen.getByText('1 个匹配条目 · 第 1 页')).toBeVisible();
  });
  it('does not pretend a failed report read succeeded and supports retry', async () => {
    const api = client();
    const user = userEvent.setup();
    vi.mocked(api.scanReport)
      .mockRejectedValueOnce({ message: '报告无法读取' })
      .mockResolvedValue(report);
    show(api);
    expect(await screen.findByRole('alert')).toHaveTextContent('报告无法读取');
    await user.click(screen.getByRole('button', { name: '重试读取' }));
    await waitFor(() => expect(screen.getByRole('table')).toBeVisible());
  });
});

it('blocks scanning while a full backup owns the workspace', () => {
  show(client(), {
    ...emptyDashboard,
    workspace,
    backup_tasks: [
      {
        id: 'backup-task',
        request_key: 'key',
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
      },
    ],
  });
  expect(screen.getByRole('button', { name: '只读扫描' })).toBeDisabled();
  expect(
    screen.getByText(/工作区正在备份、校验、恢复或检查目录/),
  ).toBeVisible();
});
