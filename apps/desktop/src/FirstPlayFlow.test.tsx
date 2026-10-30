import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { expect, it, vi } from 'vitest';
import FirstPlayFlow from './FirstPlayFlow';
import { api, emptyDashboard, type Dashboard, type Installation } from './api';
const game: Installation = {
  id: 'game',
  name: 'CDDA',
  executable: '/game/exe',
  working_dir: '/game',
  sha256: 'hash',
  version: '0.I',
  branch: 'dda',
  platform: 'macos',
  arch: 'aarch64',
  imported_at: 'now',
};
const data: Dashboard = {
  ...emptyDashboard,
  workspace: {
    id: 'w',
    root_dir: '/data',
    save_dir: '/data/saves',
    backup_dir: '/data/backups',
    content_dir: '/data/content',
    created_at: 'now',
  },
  installations: [game],
};
it('creates once, keeps completion after refresh failure, and never launches or copies saves', async () => {
  const registerManagedInstance = vi.fn().mockResolvedValue({
    id: 'new',
    name: 'CDDA',
    installation_id: 'game',
    installation_sha256: 'hash',
    data_root: '/data/instances/new',
    binding_status: 'unverified',
    created_at: 'now',
  });
  const onRefresh = vi.fn().mockRejectedValue(new Error('状态读取失败'));
  const launch = vi.fn();
  const onDone = vi.fn();
  const u = userEvent.setup();
  render(
    <FirstPlayFlow
      api={{ ...api, desktop: true, registerManagedInstance, launch }}
      data={data}
      defaults={null}
      mode="import"
      installationId="game"
      onSelectInstallation={vi.fn()}
      onImport={vi.fn()}
      onRefresh={onRefresh}
      onDone={onDone}
      onClose={vi.fn()}
    />,
  );
  expect(registerManagedInstance).not.toHaveBeenCalled();
  await u.click(screen.getByRole('button', { name: '创建独立游玩环境' }));
  expect(await screen.findByRole('alert')).toHaveTextContent('环境已经创建');
  expect(
    screen.queryByRole('button', { name: '创建独立游玩环境' }),
  ).not.toBeInTheDocument();
  expect(registerManagedInstance).toHaveBeenCalledExactlyOnceWith(
    'game',
    'CDDA',
  );
  await u.click(screen.getByRole('button', { name: '前往启动' }));
  await waitFor(() => expect(onRefresh).toHaveBeenCalledTimes(2));
  expect(onDone).not.toHaveBeenCalled();
  onRefresh.mockResolvedValue(undefined);
  await u.click(screen.getByRole('button', { name: '前往启动' }));
  await waitFor(() => expect(onDone).toHaveBeenCalledWith('new'));
  expect(registerManagedInstance).toHaveBeenCalledOnce();
  expect(launch).not.toHaveBeenCalled();
});
it('allows explicit selection of an existing installation without importing or creating', async () => {
  const onSelectInstallation = vi.fn();
  const onImport = vi.fn();
  const registerManagedInstance = vi.fn();
  render(
    <FirstPlayFlow
      api={{ ...api, desktop: true, registerManagedInstance }}
      data={data}
      defaults={null}
      mode="import"
      installationId=""
      onSelectInstallation={onSelectInstallation}
      onImport={onImport}
      onRefresh={vi.fn()}
      onDone={vi.fn()}
      onClose={vi.fn()}
    />,
  );
  await userEvent
    .setup()
    .selectOptions(screen.getByLabelText('已导入的游戏'), 'game');
  expect(onSelectInstallation).toHaveBeenCalledWith('game');
  expect(onImport).not.toHaveBeenCalled();
  expect(registerManagedInstance).not.toHaveBeenCalled();
});
