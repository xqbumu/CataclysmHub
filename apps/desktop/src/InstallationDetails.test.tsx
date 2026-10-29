import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { expect, it, vi } from 'vitest';
import InstallationDetails from './InstallationDetails';
import { emptyDashboard, type Installation, type BackupRecord } from './api';
const game: Installation = {
  id: 'game-a',
  name: 'Version A',
  version: null,
  branch: null,
  executable: '/game/cdda',
  working_dir: '/game',
  sha256: 'abc',
  platform: 'macos',
  arch: 'arm64',
  imported_at: '2026-10-29',
};
const instance = {
  id: 'instance-a',
  name: 'World A',
  installation_id: game.id,
  installation_sha256: 'abc',
  data_root: '/shared-looking/path',
  binding_status: 'unverified',
  created_at: '2026-10-29',
};
const backup: BackupRecord = {
  id: 'owned',
  play_instance_id: instance.id,
  source_root: instance.data_root,
  task_id: 'task',
  workspace_id: 'workspace',
  relative_dir: 'owned',
  created_at: '2026-10-29',
  entry_count: 1,
  file_count: 1,
  total_bytes: '1',
  consistency: 'best_effort',
  verification: 'verified',
  verified_at: '2026-10-29',
  archive_sha256: 'a',
  manifest_sha256: 'b',
  display_name: 'Owned backup',
  notes: '',
  retention_locked: false,
  metadata_revision: 0,
  metadata_updated_at: null,
};
it('associates backups by instance ID only and keeps historical compatibility uncertainty visible', async () => {
  render(
    <InstallationDetails
      installation={game}
      data={{
        ...emptyDashboard,
        play_instances: [
          instance,
          {
            ...instance,
            id: 'instance-b',
            installation_id: 'game-b',
            name: 'World B',
          },
        ],
        backups: [
          backup,
          {
            ...backup,
            id: 'unbound',
            play_instance_id: null,
            display_name: 'Unbound backup',
          },
          {
            ...backup,
            id: 'other',
            play_instance_id: 'instance-b',
            display_name: 'Other backup',
          },
        ],
      }}
      onClose={vi.fn()}
      onInstances={vi.fn()}
    />,
  );
  expect(screen.getByText(/1 个关联实例 · 1 个关联备份记录/)).toBeVisible();
  expect(screen.getByText('未知 / 未知')).toBeVisible();
  expect(
    screen.getByText(/不证明备份创建时的游戏版本或跨版本兼容性/),
  ).toBeVisible();
  await userEvent.setup().click(screen.getByText('查看 World A 的备份记录'));
  expect(screen.getByText('Owned backup')).toBeVisible();
  expect(screen.queryByText('Other backup')).not.toBeInTheDocument();
  expect(screen.queryByText('Unbound backup')).not.toBeInTheDocument();
});
it('warns about unknown sessions and mismatched registration hashes without claiming a live scan', async () => {
  const onInstances = vi.fn();
  render(
    <InstallationDetails
      installation={game}
      data={{
        ...emptyDashboard,
        play_instances: [{ ...instance, installation_sha256: 'older' }],
        sessions: [
          {
            id: 'session',
            installation_id: game.id,
            play_instance_id: instance.id,
            launch_profile_id: null,
            instance_data_root: instance.data_root,
            state: 'interrupted_unknown',
            pid: null,
            started_at: '2026-10-29',
            finished_at: null,
            exit_code: null,
          },
        ],
      }}
      onClose={vi.fn()}
      onInstances={onInstances}
    />,
  );
  expect(screen.getByText(/1 个会话状态未知/)).toBeVisible();
  expect(screen.getByText(/此处未检查磁盘上的当前程序/)).toBeVisible();
  await userEvent
    .setup()
    .click(screen.getByRole('button', { name: '管理此版本的实例' }));
  expect(onInstances).toHaveBeenCalledOnce();
});
