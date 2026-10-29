import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { it, expect, vi } from 'vitest';
import { api as native, type HubApi } from './api';
import {
  ContentInstallButton,
  ContentInstallTasks,
} from './ContentInstallWorkflow';
import type { ContentInstallPlan } from './bindings/ContentInstallPlan';
const plan: ContentInstallPlan = {
  id: 'plan',
  plan_digest: 'digest',
  entry_id: 'entry',
  source_root: '/source',
  parent_dir: '/library',
  destination: '/library/content-id',
  payload_root: '/library/content-id/data',
  content_kind: 'soundpack',
  manifest_sha256: 'a'.repeat(64),
  files: [
    { path: 'soundpack.txt', kind: 'file', size: '20', sha256: 'b'.repeat(64) },
  ],
  total_bytes: '20',
  expires_at: '2030-01-01',
  warnings: ['游戏兼容性未验证'],
};
function client(): HubApi {
  return {
    ...native,
    desktop: true,
    pickPath: vi.fn().mockResolvedValue('/library'),
    inspectContentConfig: vi.fn(),
    previewContentConfig: vi.fn(),
    prepareContentConfigProtection: vi.fn(),
    prepareConfigChange: vi.fn(),
    prepareConfigRollback: vi.fn(),
    applyConfigChange: vi.fn(),
    configChanges: vi.fn(),
    inspectConfigChange: vi.fn(),
    resolveConfigChange: vi.fn(),
    prepareContentInstall: vi.fn().mockResolvedValue(plan),
    startContentInstall: vi.fn().mockResolvedValue('task'),
    contentInstallTasks: vi.fn().mockResolvedValue([]),
    cancelContentInstall: vi.fn().mockResolvedValue(undefined),
  };
}
it('requires preview and acknowledgement and does not install when dismissed', async () => {
  const api = client();
  const user = userEvent.setup();
  const onStarted = vi.fn();
  render(
    <ContentInstallButton
      api={api}
      entryId="entry"
      unavailable={false}
      onStarted={onStarted}
    />,
  );
  await user.click(screen.getByRole('button', { name: '安装到独立资源目录' }));
  expect(api.prepareContentInstall).toHaveBeenCalledWith('entry', '/library');
  let modal = await screen.findByRole('dialog');
  expect(
    within(modal).getByRole('button', { name: '确认安装副本' }),
  ).toBeDisabled();
  expect(
    within(modal).getByText(/不写入游戏、不修改世界、不自动启用/),
  ).toBeInTheDocument();
  await user.click(within(modal).getByRole('button', { name: '取消' }));
  expect(api.startContentInstall).not.toHaveBeenCalled();
  await user.click(screen.getByRole('button', { name: '安装到独立资源目录' }));
  modal = await screen.findByRole('dialog');
  await user.click(within(modal).getByRole('checkbox'));
  await user.click(within(modal).getByRole('button', { name: '确认安装副本' }));
  expect(api.startContentInstall).toHaveBeenCalledWith(
    'plan',
    'digest',
    expect.any(String),
    true,
  );
  expect(onStarted).toHaveBeenCalledOnce();
});
it('does not prepare after picker cancellation and disables unavailable packages', async () => {
  const api = client();
  vi.mocked(api.pickPath).mockResolvedValue(null);
  const user = userEvent.setup();
  const view = render(
    <ContentInstallButton
      api={api}
      entryId="entry"
      unavailable={false}
      onStarted={vi.fn()}
    />,
  );
  await user.click(screen.getByRole('button', { name: '安装到独立资源目录' }));
  expect(api.prepareContentInstall).not.toHaveBeenCalled();
  view.rerender(
    <ContentInstallButton
      api={api}
      entryId="entry"
      unavailable
      onStarted={vi.fn()}
    />,
  );
  expect(
    screen.getByRole('button', { name: '安装到独立资源目录' }),
  ).toBeDisabled();
});
it('exposes cancellation for active tasks and keeps interrupted work distinct from success', async () => {
  const api = client();
  const user = userEvent.setup();
  const base = {
    id: 'task',
    entry_id: 'entry',
    source_root: '/source',
    destination: '/library/copy',
    payload_root: '/library/copy/data',
    manifest_sha256: 'a'.repeat(64),
    state: 'running',
    phase: 'copying',
    revision: 1,
    error_code: null,
    created_at: 'now',
    finished_at: null,
  };
  vi.mocked(api.contentInstallTasks).mockResolvedValue([
    base,
    {
      ...base,
      id: 'old',
      state: 'recovery_required',
      error_code: 'TASK_INTERRUPTED',
    },
  ]);
  render(<ContentInstallTasks api={api} revision={0} />);
  await user.click(await screen.findByRole('button', { name: '取消安装' }));
  expect(api.cancelContentInstall).toHaveBeenCalledWith('task');
  expect(
    screen.getByText(/任务中断或结果记录失败，未自动重放/),
  ).toBeInTheDocument();
  expect(screen.queryByText(/副本校验完成/)).not.toBeInTheDocument();
});
