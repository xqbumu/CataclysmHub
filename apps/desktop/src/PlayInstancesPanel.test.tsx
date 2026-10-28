import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { expect, it, vi } from 'vitest';
import PlayInstancesPanel from './PlayInstancesPanel';
import { api, emptyDashboard } from './api';

it('requires a native selection and keeps binding uncertainty explicit', async () => {
  const user = userEvent.setup();
  const registerPlayInstance = vi.fn().mockResolvedValue({});
  const onRefresh = vi.fn().mockResolvedValue(undefined);
  render(
    <PlayInstancesPanel
      api={{
        ...api,
        desktop: true,
        pickPath: vi.fn().mockResolvedValue('/isolated'),
        registerPlayInstance,
      }}
      data={{
        ...emptyDashboard,
        installations: [
          {
            id: 'game',
            name: 'Game A',
            executable: '/game/exe',
            working_dir: '/game',
            sha256: 'abc',
            version: null,
            branch: null,
            platform: 'test',
            arch: 'test',
            imported_at: '',
          },
        ],
      }}
      onRefresh={onRefresh}
    />,
  );
  expect(screen.getByText(/并行启动尚未开放/)).toBeInTheDocument();
  expect(screen.getByRole('button', { name: '登记实例' })).toBeDisabled();
  await user.selectOptions(screen.getByLabelText('关联安装'), 'game');
  await user.type(screen.getByLabelText('实例名称'), '独立世界');
  await user.click(screen.getByRole('button', { name: '选择实例空目录' }));
  await user.click(screen.getByRole('button', { name: '登记实例' }));
  expect(registerPlayInstance).toHaveBeenCalledWith({
    installation_id: 'game',
    name: '独立世界',
    data_root: '/isolated',
  });
  expect(onRefresh).toHaveBeenCalled();
});

it('requires choosing a trial profile and a separate confirmation before launch', async () => {
  const user = userEvent.setup();
  const instanceLaunchPlan = vi.fn().mockResolvedValue({
    id: 'plan',
    play_instance_id: 'instance',
    launch_profile_id: 'cdda-0h-source-trial-v1',
    instance_data_root: '/isolated',
    installation_id: 'game',
    executable: '/game/exe',
    working_dir: '/game',
    args: ['--userdir', '/isolated/'],
    executable_sha256: 'abc',
    save_dir: '/isolated/save',
    save_binding_verified: false,
    expires_at: '2030-01-01',
    warnings: ['仍可能写入安装缓存，不允许并发'],
  });
  const launch = vi.fn().mockResolvedValue({});
  render(
    <PlayInstancesPanel
      api={{ ...api, desktop: true, instanceLaunchPlan, launch }}
      data={{
        ...emptyDashboard,
        play_instances: [
          {
            id: 'instance',
            installation_id: 'game',
            installation_sha256: 'abc',
            name: '独立实例',
            data_root: '/isolated',
            binding_status: 'unverified',
            created_at: '',
          },
        ],
      }}
      onRefresh={vi.fn().mockResolvedValue(undefined)}
    />,
  );
  const preview = screen.getByRole('button', { name: '预览 独立实例 试运行' });
  expect(preview).toBeDisabled();
  await user.click(screen.getByRole('checkbox'));
  await user.click(preview);
  expect(instanceLaunchPlan).toHaveBeenCalledWith(
    'instance',
    'cdda-0h-source-trial-v1',
  );
  expect(launch).not.toHaveBeenCalled();
  expect(
    screen.getByText('仍可能写入安装缓存，不允许并发'),
  ).toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: '取消' }));
  expect(launch).not.toHaveBeenCalled();
  await user.click(preview);
  await user.click(screen.getByRole('button', { name: '确认单实例试运行' }));
  expect(launch).toHaveBeenCalledWith('plan');
});
