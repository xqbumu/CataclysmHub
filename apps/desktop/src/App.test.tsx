import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi } from 'vitest';
import App from './App';
import {
  emptyDashboard,
  parseArguments,
  type HubApi,
  type Dashboard,
  type Installation,
  type LaunchPlan,
} from './api';
const workspace = {
  id: 'workspace',
  save_dir: '/selected/saves',
  backup_dir: '/selected/backups',
  content_dir: '/selected/content',
  created_at: '2026-10-27T00:00:00Z',
};
const game: Installation = {
  id: 'game',
  name: '本地 CDDA',
  executable: '/games/cdda',
  working_dir: '/games',
  sha256: 'a'.repeat(64),
  version: null,
  branch: null,
  platform: 'macos',
  arch: 'aarch64',
  imported_at: workspace.created_at,
};
const plan: LaunchPlan = {
  id: 'plan',
  installation_id: game.id,
  executable: game.executable,
  working_dir: game.working_dir,
  args: [],
  executable_sha256: game.sha256,
  save_dir: workspace.save_dir,
  save_binding_verified: false,
  expires_at: '2030-01-01T00:00:00Z',
  warnings: ['尚未验证实际存档路径。'],
};
function client(data: Dashboard = emptyDashboard): HubApi {
  return {
    desktop: true,
    dashboard: vi.fn().mockResolvedValue(data),
    pickPath: vi.fn(),
    initialize: vi.fn(),
    importInstallation: vi.fn(),
    launchPlan: vi.fn().mockResolvedValue(plan),
    launch: vi.fn(),
    acknowledge: vi.fn(),
    scanWorlds: vi.fn(),
    cancelScan: vi.fn(),
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
    scanReport: vi.fn(),
  };
}
describe('workspace and launch safety', () => {
  it('does not invent backup health or enable unimplemented actions', async () => {
    render(<App api={client()} />);
    expect(await screen.findByText('建立你的安全工作区')).toBeVisible();
    expect(screen.getByRole('button', { name: /存档历史/ })).toBeDisabled();
    expect(screen.getByRole('button', { name: /导入游戏/ })).toBeDisabled();
    expect(screen.getByRole('button', { name: /保存工作区/ })).toBeDisabled();
  });
  it('requires three native directory selections before initialization', async () => {
    const api = client();
    const user = userEvent.setup();
    vi.mocked(api.pickPath)
      .mockResolvedValueOnce('/selected/saves')
      .mockResolvedValueOnce('/selected/backups')
      .mockResolvedValueOnce('/selected/content');
    vi.mocked(api.initialize).mockResolvedValue(workspace);
    render(<App api={api} />);
    await screen.findByText('建立你的安全工作区');
    for (const button of screen.getAllByRole('button', { name: '选择' }))
      await user.click(button);
    await user.click(screen.getByRole('button', { name: /保存工作区/ }));
    await waitFor(() =>
      expect(api.initialize).toHaveBeenCalledWith({
        save_dir: workspace.save_dir,
        backup_dir: workspace.backup_dir,
        content_dir: workspace.content_dir,
      }),
    );
  });
  it('never launches just by opening or generating a preview; cancellation is side-effect free', async () => {
    const api = client({ ...emptyDashboard, workspace, installations: [game] });
    const user = userEvent.setup();
    render(<App api={api} />);
    await user.click(await screen.findByRole('button', { name: /启动预览/ }));
    const dialog = screen.getByRole('dialog');
    await user.click(
      within(dialog).getByRole('button', { name: /生成启动预览/ }),
    );
    expect(await screen.findByText('尚未验证实际存档路径。')).toBeVisible();
    expect(api.launch).not.toHaveBeenCalled();
    await user.click(within(dialog).getByRole('button', { name: '取消' }));
    expect(api.launch).not.toHaveBeenCalled();
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });
  it('requires explicit confirmation after preview before launch', async () => {
    const api = client({ ...emptyDashboard, workspace, installations: [game] });
    const user = userEvent.setup();
    render(<App api={api} />);
    await user.click(await screen.findByRole('button', { name: /启动预览/ }));
    await user.click(screen.getByRole('button', { name: /生成启动预览/ }));
    await user.click(await screen.findByRole('button', { name: '确认启动' }));
    await waitFor(() =>
      expect(api.launch).toHaveBeenCalledExactlyOnceWith('plan'),
    );
  });
  it('keeps rejected workspace selections visible and surfaces the backend error', async () => {
    const api = client();
    const user = userEvent.setup();
    vi.mocked(api.pickPath).mockResolvedValue('/same');
    vi.mocked(api.initialize).mockRejectedValue({
      code: 'PATH_OVERLAP',
      message: '目录不能重叠',
    });
    render(<App api={api} />);
    await screen.findByText('建立你的安全工作区');
    for (const button of screen.getAllByRole('button', { name: '选择' }))
      await user.click(button);
    await user.click(screen.getByRole('button', { name: /保存工作区/ }));
    expect(await screen.findByRole('alert')).toHaveTextContent('目录不能重叠');
    expect(screen.getByLabelText('存档根目录')).toHaveValue('/same');
  });
  it('validates literal argument arrays without accepting shell strings', () => {
    expect(parseArguments('["$(touch marker)","with spaces"]')).toEqual([
      '$(touch marker)',
      'with spaces',
    ]);
    for (const bad of ['"--flag"', '{}', '[1]', '["\\u0000"]'])
      expect(() => parseArguments(bad)).toThrow();
  });
  it('does not label a failed directory inspection as a failed game launch', async () => {
    render(
      <App
        api={client({
          ...emptyDashboard,
          workspace,
          operations: [
            {
              id: 'event',
              task_id: 'inspection',
              action: 'backup.inspect_restored',
              status: 'failed',
              error_code: 'RESTORED_DATA_MISMATCH',
              created_at: workspace.created_at,
            },
          ],
        })}
      />,
    );
    await userEvent
      .setup()
      .click(await screen.findByRole('button', { name: '操作记录' }));
    const table = screen.getByRole('table');
    expect(within(table).queryByText('启动失败')).not.toBeInTheDocument();
    expect(within(table).getByText('失败')).toBeVisible();
    expect(within(table).getByText('检查已恢复目录')).toBeVisible();
  });
});
