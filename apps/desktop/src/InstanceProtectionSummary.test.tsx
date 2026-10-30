import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { expect, it, vi } from 'vitest';
import InstanceProtectionSummary from './InstanceProtectionSummary';
import { emptyDashboard, type HubApi } from './api';
import type { AutomaticBackupPolicy } from './bindings/AutomaticBackupPolicy';
const policy: AutomaticBackupPolicy = {
  instance_id: 'a',
  revision: 1,
  settings: {
    enabled: true,
    min_interval_minutes: 15,
    retention: { keep_recent: 7, daily_days: 14, weekly_weeks: 8 },
  },
  updated_at: 'now',
  last_status: 'skipped',
  last_error_code: 'AUTO_BACKUP_EMPTY_SAVE',
  last_task_id: null,
};

it('explains skipped progress backup and routes manual retry without copying anything', async () => {
  const open = vi.fn();
  const api = {
    automaticBackupPolicy: vi.fn().mockResolvedValue(policy),
  } as unknown as HubApi;
  render(
    <InstanceProtectionSummary
      api={api}
      data={emptyDashboard}
      instanceId="a"
      onOpenBackups={open}
    />,
  );
  expect(await screen.findByText(/尚无已保存的游戏进度/)).toBeInTheDocument();
  expect(screen.getByText(/已开启 · 最小间隔 15 分钟/)).toBeInTheDocument();
  await userEvent.click(screen.getByRole('button', { name: '前往手动备份' }));
  expect(open).toHaveBeenCalledWith('a');
});

it('does not treat failed reads as disabled and rejects a late response from a different instance', async () => {
  let finish!: (value: AutomaticBackupPolicy) => void;
  const api = {
    automaticBackupPolicy: vi.fn((id: string) =>
      id === 'a'
        ? new Promise<AutomaticBackupPolicy>((r) => {
            finish = r;
          })
        : Promise.reject(new Error('offline')),
    ),
  } as unknown as HubApi;
  const view = render(
    <InstanceProtectionSummary
      api={api}
      data={emptyDashboard}
      instanceId="a"
    />,
  );
  view.rerender(
    <InstanceProtectionSummary
      api={api}
      data={emptyDashboard}
      instanceId="b"
    />,
  );
  expect(
    await screen.findByText('退出后自动备份：状态未知'),
  ).toBeInTheDocument();
  finish(policy);
  expect(screen.queryByText(/已开启/)).not.toBeInTheDocument();
  expect(
    screen.getByRole('button', { name: '重新读取保护状态' }),
  ).toBeInTheDocument();
});

it('refreshes for automatic skip events with no task and blocks retry while resources are claimed', async () => {
  const read = vi
    .fn()
    .mockResolvedValue({ ...policy, last_status: null, last_error_code: null });
  const api = { automaticBackupPolicy: read } as unknown as HubApi;
  const data = {
    ...emptyDashboard,
    sessions: [
      {
        id: 'session',
        installation_id: 'game',
        play_instance_id: 'a',
        state: 'exited_unverified',
        pid: null,
        started_at: 'now',
        finished_at: 'now',
        exit_code: 0,
        launch_profile_id: null,
        instance_data_root: '/a',
      },
    ],
  };
  const view = render(
    <InstanceProtectionSummary
      api={api}
      data={data}
      instanceId="a"
      onOpenBackups={vi.fn()}
    />,
  );
  await screen.findByText(/尚无自动备份记录/);
  read.mockResolvedValue(policy);
  view.rerender(
    <InstanceProtectionSummary
      api={api}
      data={{
        ...data,
        operations: [
          {
            id: 'event',
            task_id: 'session',
            action: 'backup.automatic',
            status: 'skipped',
            error_code: 'AUTO_BACKUP_EMPTY_SAVE',
            created_at: 'now',
          },
        ],
      }}
      instanceId="a"
      onOpenBackups={vi.fn()}
    />,
  );
  expect(await screen.findByText(/本次未生成副本/)).toBeInTheDocument();
  await waitFor(() => expect(read).toHaveBeenCalledTimes(2));
  view.rerender(
    <InstanceProtectionSummary
      api={api}
      data={{
        ...data,
        resource_claims: [
          {
            owner_kind: 'session',
            owner_id: 'session',
            state: 'running',
            scope: 'global',
            operation: 'game',
            access: 'write',
            path: null,
            reason: 'unknown',
          },
        ],
      }}
      instanceId="a"
      onOpenBackups={vi.fn()}
    />,
  );
  expect(
    await screen.findByRole('button', { name: '前往手动备份' }),
  ).toBeDisabled();
});
