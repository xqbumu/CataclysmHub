import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { expect, it, vi } from 'vitest';
import InstallationCopiesPanel from './InstallationCopiesPanel';
import { api, emptyDashboard } from './api';
import type { InstallationCopyTask } from './bindings/InstallationCopyTask';
import type { ManagedInstallationCopyPlan } from './bindings/ManagedInstallationCopyPlan';
const installation = {
  id: 'game',
  name: 'Source',
  executable: '/game/test.exe',
  working_dir: '/game',
  sha256: 'a'.repeat(64),
  version: null,
  branch: null,
  platform: 'test',
  arch: 'test',
  imported_at: '2026-10-28T00:00:00Z',
};
const plan: ManagedInstallationCopyPlan = {
  id: 'plan',
  plan_digest: 'digest',
  source_installation_id: 'game',
  source_root: '/game',
  parent_dir: '/copies',
  destination: '/copies/new-copy',
  executable_relative: 'test.exe',
  executable_sha256: 'a'.repeat(64),
  copy_id: 'copy',
  payload_root: 'payload/game',
  manifest_sha256: 'b'.repeat(64),
  entry_count: 1,
  file_count: 1,
  link_count: 0,
  total_bytes: '4',
  expires_at: '2030-01-01T00:00:00Z',
  warnings: ['不会自动绑定或执行游戏。'],
};
const task: InstallationCopyTask = {
  id: 'task',
  request_key: 'key',
  source_installation_id: 'game',
  source_root: '/game',
  parent_dir: '/copies',
  destination: '/copies/new-copy',
  executable_relative: 'test.exe',
  executable_sha256: 'a'.repeat(64),
  copy_id: 'copy',
  payload_root: 'payload/game',
  manifest_sha256: 'b'.repeat(64),
  entry_count: 1,
  file_count: 1,
  link_count: 0,
  total_bytes: '4',
  state: 'succeeded',
  phase: 'finished',
  processed_entries: 1,
  revision: 1,
  error_code: null,
  created_at: '2026-10-28T00:00:00Z',
  finished_at: '2026-10-28T00:01:00Z',
  verified_at: '2026-10-28T00:01:00Z',
  registered_installation_id: null,
};
const data = { ...emptyDashboard, installations: [installation] };
it('requires native scope selection and separate copy confirmation; preview cancellation never starts work', async () => {
  const user = userEvent.setup();
  const start = vi.fn().mockResolvedValue({ ...task, state: 'running' });
  const prepare = vi.fn().mockResolvedValue(plan);
  const register = vi.fn();
  const client = {
    ...api,
    desktop: true,
    pickPath: vi
      .fn()
      .mockResolvedValueOnce('/game')
      .mockResolvedValueOnce('/copies'),
    inspectContentConfig: vi.fn(),
    previewContentConfig: vi.fn(),
    prepareContentConfigProtection: vi.fn(),
    prepareConfigChange: vi.fn(),
    prepareConfigRollback: vi.fn(),
    applyConfigChange: vi.fn(),
    configChanges: vi.fn(),
    inspectConfigChange: vi.fn(),
    resolveConfigChange: vi.fn(),
    prepareContentInstall: vi.fn(),
    startContentInstall: vi.fn(),
    contentInstallTasks: vi.fn().mockResolvedValue([]),
    cancelContentInstall: vi.fn(),
    contentEntries: vi.fn().mockResolvedValue([]),
    registerContent: vi.fn(),
    refreshContent: vi.fn(),
    forgetContent: vi.fn(),
    prepareInstallationCopy: prepare,
    startInstallationCopy: start,
    registerCopiedInstallation: register,
  };
  render(
    <InstallationCopiesPanel
      api={client}
      data={data}
      onRefresh={vi.fn().mockResolvedValue(undefined)}
    />,
  );
  expect(
    screen.getByRole('button', { name: '预览独立安装复制' }),
  ).toBeDisabled();
  await user.selectOptions(screen.getByLabelText('来源安装'), 'game');
  await user.click(screen.getByRole('button', { name: '选择完整安装目录' }));
  await user.click(screen.getByRole('button', { name: '选择副本存放位置' }));
  await user.click(screen.getByRole('button', { name: '预览独立安装复制' }));
  expect(prepare).toHaveBeenCalledWith('game', '/game', '/copies');
  let modal = await screen.findByRole('dialog');
  expect(within(modal).getByText('/copies/new-copy')).toBeInTheDocument();
  expect(
    within(modal).getByRole('button', { name: '确认复制到新目录' }),
  ).toBeDisabled();
  await user.click(within(modal).getByRole('button', { name: '取消预览' }));
  expect(start).not.toHaveBeenCalled();
  await user.click(screen.getByRole('button', { name: '预览独立安装复制' }));
  modal = await screen.findByRole('dialog');
  await user.click(within(modal).getByRole('checkbox'));
  await user.click(
    within(modal).getByRole('button', { name: '确认复制到新目录' }),
  );
  expect(start).toHaveBeenCalledWith(
    'plan',
    'digest',
    expect.any(String),
    true,
  );
  expect(register).not.toHaveBeenCalled();
});
it('registers only after a separate explicit action and retains interruption context', async () => {
  const user = userEvent.setup();
  const register = vi.fn().mockResolvedValue({ ...installation, id: 'new' });
  const start = vi.fn();
  const client = {
    ...api,
    desktop: true,
    registerCopiedInstallation: register,
    startInstallationCopy: start,
  };
  const refresh = vi.fn().mockResolvedValue(undefined);
  render(
    <InstallationCopiesPanel
      api={client}
      data={{
        ...data,
        copy_tasks: [
          {
            ...task,
            state: 'recovery_required',
            phase: 'interrupted',
            error_code: 'TASK_INTERRUPTED',
          },
        ],
      }}
      onRefresh={refresh}
    />,
  );
  expect(screen.getByText(/不会自动重放复制/)).toBeInTheDocument();
  expect(register).not.toHaveBeenCalled();
  await user.click(screen.getByRole('button', { name: '核验并登记副本' }));
  const modal = await screen.findByRole('dialog');
  await user.clear(within(modal).getByLabelText('新安装名称'));
  await user.type(within(modal).getByLabelText('新安装名称'), 'New Copy');
  await user.click(
    within(modal).getByRole('button', { name: '确认核验并登记' }),
  );
  expect(register).toHaveBeenCalledWith('task', 'New Copy');
  expect(start).not.toHaveBeenCalled();
  expect(refresh).toHaveBeenCalled();
});
it('allows cancellation while copying and disables it at the publication boundary', async () => {
  const user = userEvent.setup();
  const cancel = vi.fn().mockResolvedValue({ ...task, state: 'cancelling' });
  const client = { ...api, desktop: true, cancelInstallationCopy: cancel };
  const refresh = vi.fn().mockResolvedValue(undefined);
  const view = render(
    <InstallationCopiesPanel
      api={client}
      data={{
        ...data,
        copy_tasks: [{ ...task, state: 'running', phase: 'copying' }],
      }}
      onRefresh={refresh}
    />,
  );
  await user.click(screen.getByRole('button', { name: '取消复制' }));
  expect(cancel).toHaveBeenCalledWith('task');
  view.rerender(
    <InstallationCopiesPanel
      api={client}
      data={{
        ...data,
        copy_tasks: [{ ...task, state: 'running', phase: 'publishing' }],
      }}
      onRefresh={refresh}
    />,
  );
  expect(screen.getByRole('button', { name: '取消复制' })).toBeDisabled();
  expect(
    screen.queryByRole('button', { name: '核验并登记副本' }),
  ).not.toBeInTheDocument();
});
