import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { expect, it, vi } from 'vitest';
import PlayInstancesPanel from './PlayInstancesPanel';
import { api, emptyDashboard } from './api';

async function openLaunch(
  user: ReturnType<typeof userEvent.setup>,
  name: string,
) {
  await user.click(screen.getByRole('button', { name: '启动 ' + name }));
  await user.click(await screen.findByText('高级启动方案（旧版与实验选项）'));
  await screen.findByLabelText('试运行参数方案');
  await user.selectOptions(
    screen.getByLabelText('试运行参数方案'),
    'cdda-0h-source-trial-v1',
  );
}

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
  expect(screen.getByText(/普通方案保持全局互斥/)).toBeInTheDocument();
  expect(screen.getByRole('button', { name: '登记实例' })).toBeDisabled();
  await user.selectOptions(screen.getByLabelText('关联安装'), 'game');
  await user.type(screen.getByLabelText('实例名称'), '独立世界');
  await user.selectOptions(screen.getByLabelText('实例目录方式'), 'custom');
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
    environment: {},
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
  await openLaunch(user, '独立实例');
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
  await user.click(
    screen.getByRole('button', { name: '预览 独立实例 试运行' }),
  );
  await user.click(screen.getByRole('button', { name: '确认启动' }));
  expect(launch).toHaveBeenCalledWith('plan');
});

it('changing a version profile clears acknowledgement and forwards the selected profile', async () => {
  const user = userEvent.setup();
  const instanceLaunchPlan = vi.fn().mockResolvedValue({
    environment: { DYLD_FRAMEWORK_PATH: '/Game.app/Contents/Resources' },
    id: 'plan',
    executable: '/Game.app/Contents/Resources/cataclysm-tiles',
    working_dir: '/Game.app/Contents/Resources',
    instance_data_root: '/isolated',
    save_dir: '/isolated/save',
    args: [],
    executable_sha256: 'abc',
    warnings: ['未验证'],
  });
  render(
    <PlayInstancesPanel
      api={{ ...api, desktop: true, instanceLaunchPlan }}
      data={{
        ...emptyDashboard,
        play_instances: [
          {
            id: 'instance',
            installation_id: 'game',
            installation_sha256: 'abc',
            name: '测试',
            data_root: '/isolated',
            binding_status: 'unverified',
            created_at: '',
          },
        ],
      }}
      onRefresh={vi.fn()}
    />,
  );
  await openLaunch(user, '测试');
  await user.click(screen.getByRole('checkbox'));
  await user.selectOptions(
    screen.getByLabelText('试运行参数方案'),
    'cdda-0i-1-macos-trial-v1',
  );
  expect(screen.getByRole('checkbox')).not.toBeChecked();
  expect(
    screen.getByRole('button', { name: '预览 测试 试运行' }),
  ).toBeDisabled();
  await user.click(screen.getByRole('checkbox'));
  await user.click(screen.getByRole('button', { name: '预览 测试 试运行' }));
  expect(instanceLaunchPlan).toHaveBeenCalledWith(
    'instance',
    'cdda-0i-1-macos-trial-v1',
  );
  expect(
    screen.getByText('{"DYLD_FRAMEWORK_PATH":"/Game.app/Contents/Resources"}'),
  ).toBeInTheDocument();
});

it('can inspect conflicts while launch is blocked without choosing a profile or launching', async () => {
  const user = userEvent.setup();
  const previewInstanceResources = vi.fn().mockResolvedValue({
    instance_id: 'instance',
    checked_at: '2026-10-28T00:00:00Z',
    scopes: [{ role: 'installation', path: '/shared/game', access: 'write' }],
    scope_issues: [],
    conflicts: [
      {
        owner_kind: 'session',
        owner_id: 'other-instance-session',
        operation: 'game.launch',
        state: 'running',
        reason: 'directory_overlap',
        requested_path: '/shared/game',
        protected_path: '/shared/game',
      },
    ],
    binding_verified: false,
    concurrency_available: false,
  });
  const launch = vi.fn();
  render(
    <PlayInstancesPanel
      api={{ ...api, desktop: true, previewInstanceResources, launch }}
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
        resource_claims: [
          {
            owner_kind: 'session',
            owner_id: 'other-instance-session',
            operation: 'game.launch',
            state: 'running',
            scope: 'global',
            access: 'exclusive',
            path: null,
            reason: 'unknown_game_writes',
          },
        ],
      }}
      onRefresh={vi.fn()}
    />,
  );
  expect(screen.getByRole('button', { name: '启动 独立实例' })).toBeEnabled();
  await user.click(screen.getByText('资源冲突诊断'));
  await user.click(
    screen.getByRole('button', { name: '检查 独立实例 资源冲突' }),
  );
  expect(previewInstanceResources).toHaveBeenCalledWith('instance');
  expect(
    await screen.findByText('物理目录相同或互相嵌套，存在读写冲突'),
  ).toBeInTheDocument();
  expect(screen.getByText('other-instance-session')).toBeInTheDocument();
  expect(screen.getByText(/不预留资源/)).toBeInTheDocument();
  expect(launch).not.toHaveBeenCalled();
  await user.click(screen.getByRole('button', { name: '关闭检查结果' }));
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  // A new observation replaces the old conflict; it still does not authorize launch.
  previewInstanceResources.mockResolvedValueOnce({
    instance_id: 'instance',
    checked_at: '2026-10-28T00:01:00Z',
    scopes: [],
    scope_issues: [{ path: '/missing', code: 'RESOURCE_SCOPE_UNAVAILABLE' }],
    conflicts: [],
    binding_verified: false,
    concurrency_available: false,
  });
  await user.click(
    screen.getByRole('button', { name: '检查 独立实例 资源冲突' }),
  );
  expect(
    await screen.findByText('目录存在未解决问题，不能据此判断可启动。'),
  ).toBeInTheDocument();
  expect(screen.queryByText('other-instance-session')).not.toBeInTheDocument();
  expect(launch).not.toHaveBeenCalled();
});

it('lets an explicit candidate request backend admission while other claims exist', async () => {
  const user = userEvent.setup();
  const instanceLaunchPlan = vi
    .fn()
    .mockRejectedValue(new Error('candidate unavailable'));
  render(
    <PlayInstancesPanel
      api={{ ...api, desktop: true, instanceLaunchPlan }}
      data={{
        ...emptyDashboard,
        play_instances: [
          {
            id: 'instance',
            name: 'Candidate',
            installation_id: 'game',
            installation_sha256: 'hash',
            data_root: '/instance',
            binding_status: 'unverified',
            created_at: '',
          },
        ],
        resource_claims: [
          {
            owner_kind: 'session',
            owner_id: 'other',
            operation: 'game.launch',
            state: 'running',
            scope: 'directory',
            access: 'write',
            path: '/other',
            reason: 'declared_directory',
          },
        ],
      }}
      onRefresh={vi.fn()}
    />,
  );
  await openLaunch(user, 'Candidate');
  await user.click(screen.getByRole('checkbox'));
  expect(
    screen.getByRole('button', { name: '预览 Candidate 试运行' }),
  ).toBeDisabled();
  await user.selectOptions(
    screen.getByLabelText('试运行参数方案'),
    'cdda-0i-1-macos-concurrency-candidate-v1',
  );
  expect(
    screen.getByRole('button', { name: '预览 Candidate 试运行' }),
  ).toBeDisabled();
  await user.click(screen.getByRole('checkbox'));
  await user.click(
    screen.getByRole('button', { name: '预览 Candidate 试运行' }),
  );
  expect(instanceLaunchPlan).toHaveBeenCalledWith(
    'instance',
    'cdda-0i-1-macos-concurrency-candidate-v1',
  );
  expect(await screen.findByRole('alert')).toBeInTheDocument();
});

it('keeps saved launch profiles and acknowledgement scoped to one instance', async () => {
  const user = userEvent.setup();
  const saveInstanceProfile = vi.fn().mockResolvedValue({
    revision: 2,
    instance_profiles: {
      a: 'cdda-0h-source-trial-v1',
      b: 'cdda-0h-source-trial-v1',
    },
  });
  const instanceLaunchPlan = vi.fn();
  const base = {
    id: 'a',
    name: 'A',
    installation_id: 'game',
    installation_sha256: 'hash',
    data_root: '/a',
    binding_status: 'unverified',
    created_at: '',
  };
  render(
    <PlayInstancesPanel
      api={{
        ...api,
        desktop: true,
        preferences: vi.fn().mockResolvedValue({
          revision: 1,
          instance_profiles: { a: 'cdda-0h-source-trial-v1' },
        }),
        saveInstanceProfile,
        instanceLaunchPlan,
      }}
      data={{
        ...emptyDashboard,
        play_instances: [
          base,
          { ...base, id: 'b', name: 'B', data_root: '/b' },
        ],
      }}
      onRefresh={vi.fn()}
    />,
  );
  await user.click(screen.getByRole('button', { name: '启动设置 A' }));
  await user.click(await screen.findByText('高级启动方案（旧版与实验选项）'));
  expect(await screen.findByLabelText('试运行参数方案')).toHaveValue(
    'cdda-0h-source-trial-v1',
  );
  await user.click(screen.getByRole('checkbox'));
  await user.click(screen.getByRole('button', { name: '关闭对话框' }));
  await user.click(screen.getByRole('button', { name: '启动设置 B' }));
  await user.click(await screen.findByText('高级启动方案（旧版与实验选项）'));
  expect(await screen.findByLabelText('试运行参数方案')).toHaveValue('');
  expect(screen.getByRole('checkbox')).not.toBeChecked();
  expect(screen.getByRole('button', { name: '预览 B 试运行' })).toBeDisabled();
  await user.selectOptions(
    screen.getByLabelText('试运行参数方案'),
    'cdda-0h-source-trial-v1',
  );
  await user.click(screen.getByRole('button', { name: '保存此实例启动方案' }));
  expect(saveInstanceProfile).toHaveBeenCalledWith(
    'b',
    'cdda-0h-source-trial-v1',
  );
  expect(instanceLaunchPlan).not.toHaveBeenCalled();
});

it('starts a configured instance through a fresh backend plan without reopening settings', async () => {
  const user = userEvent.setup();
  const launch = vi.fn().mockResolvedValue({});
  const instanceLaunchPlan = vi.fn().mockResolvedValue({ id: 'fresh' });
  const onRefresh = vi.fn();
  render(
    <PlayInstancesPanel
      home
      api={{
        ...api,
        desktop: true,
        preferences: vi.fn().mockResolvedValue({
          revision: 1,
          instance_profiles: { a: 'cdda-0i-1-macos-native-trial-v1' },
        }),
        instanceLaunchPlan,
        launch,
      }}
      data={{
        ...emptyDashboard,
        play_instances: [
          {
            id: 'a',
            name: 'A',
            installation_id: 'g',
            installation_sha256: 'hash',
            data_root: '/a',
            binding_status: 'unverified',
            created_at: '',
          },
        ],
      }}
      onRefresh={onRefresh}
    />,
  );
  await user.click(screen.getByRole('button', { name: '启动 A' }));
  await vi.waitFor(() => expect(launch).toHaveBeenCalledWith('fresh'));
  expect(instanceLaunchPlan).toHaveBeenCalledWith(
    'a',
    'cdda-0i-1-macos-native-trial-v1',
  );
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
});
it('does not launch when a saved profile fails current backend checks', async () => {
  const user = userEvent.setup();
  const launch = vi.fn();
  render(
    <PlayInstancesPanel
      api={{
        ...api,
        desktop: true,
        preferences: vi.fn().mockResolvedValue({
          revision: 1,
          instance_profiles: { a: 'cdda-0i-1-macos-native-trial-v1' },
        }),
        instanceLaunchPlan: vi
          .fn()
          .mockRejectedValue(new Error('游戏程序已变化')),
        launch,
      }}
      data={{
        ...emptyDashboard,
        play_instances: [
          {
            id: 'a',
            name: 'A',
            installation_id: 'g',
            installation_sha256: 'hash',
            data_root: '/a',
            binding_status: 'unverified',
            created_at: '',
          },
        ],
      }}
      onRefresh={vi.fn()}
    />,
  );
  await user.click(screen.getByRole('button', { name: '启动 A' }));
  expect(await screen.findByRole('alert')).toHaveTextContent('游戏程序已变化');
  expect(launch).not.toHaveBeenCalled();
});

it.each([false, true])(
  'prefills matching recognition without launching (saved=%s)',
  async (saved) => {
    const user = userEvent.setup();
    const launch = vi.fn();
    const instanceLaunchPlan = vi.fn();
    render(
      <PlayInstancesPanel
        api={{
          ...api,
          desktop: true,
          preferences: vi.fn().mockResolvedValue({
            revision: 0,
            instance_profiles: saved
              ? { a: 'cdda-0i-1-macos-native-trial-v1' }
              : {},
          }),
          inspectInstallationRecognition: vi.fn().mockResolvedValue({
            installation_id: 'g',
            executable_sha256: 'hash',
            version: '0.I-1',
            recommended_profile: 'cdda-0i-1-macos-native-trial-v1',
            evidence: '匹配已记录样本',
          }),
          instanceLaunchPlan,
          launch,
        }}
        data={{
          ...emptyDashboard,
          play_instances: [
            {
              id: 'a',
              name: 'A',
              installation_id: 'g',
              installation_sha256: 'hash',
              data_root: '/a',
              binding_status: 'unverified',
              created_at: '',
            },
          ],
        }}
        onRefresh={vi.fn()}
      />,
    );
    await user.click(screen.getByRole('button', { name: '启动设置 A' }));
    expect(await screen.findByRole('status')).toHaveTextContent(
      '识别版本：CDDA 0.I-1',
    );
    expect(screen.queryByLabelText('游戏版本')).not.toBeInTheDocument();
    expect(
      screen.getByText('启动适配：自动。无需手动选择游戏版本或参数方案。'),
    ).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '预览 A 试运行' })).toBeEnabled();
    expect(launch).not.toHaveBeenCalled();
    expect(instanceLaunchPlan).not.toHaveBeenCalled();
  },
);
it('does not prefill a recommendation belonging to another installation', async () => {
  const user = userEvent.setup();
  render(
    <PlayInstancesPanel
      api={{
        ...api,
        desktop: true,
        preferences: vi
          .fn()
          .mockResolvedValue({ revision: 0, instance_profiles: {} }),
        inspectInstallationRecognition: vi.fn().mockResolvedValue({
          installation_id: 'other',
          executable_sha256: 'hash',
          version: '0.I-1',
          recommended_profile: 'cdda-0i-1-macos-native-trial-v1',
          evidence: '匹配',
        }),
      }}
      data={{
        ...emptyDashboard,
        play_instances: [
          {
            id: 'a',
            name: 'A',
            installation_id: 'g',
            installation_sha256: 'hash',
            data_root: '/a',
            binding_status: 'unverified',
            created_at: '',
          },
        ],
      }}
      onRefresh={vi.fn()}
    />,
  );
  await user.click(screen.getByRole('button', { name: '启动 A' }));
  expect(
    await screen.findByText('安装记录与实例不一致，请重新核对安装。'),
  ).toBeInTheDocument();
  expect(screen.queryByLabelText('游戏版本')).not.toBeInTheDocument();
});

it('opens a first recognized launch directly at confirmation and never launches on cancel', async () => {
  const user = userEvent.setup();
  const instanceLaunchPlan = vi.fn().mockResolvedValue({
    id: 'automatic',
    environment: {},
    executable: '/Game.app/Contents/MacOS/game',
    working_dir: '/Game.app/Contents/Resources',
    instance_data_root: '/instance',
    save_dir: '/instance/save',
    args: [],
    executable_sha256: 'hash',
    warnings: ['详细适配警告'],
  });
  const launch = vi.fn();
  const saveInstanceProfile = vi.fn();
  render(
    <PlayInstancesPanel
      api={{
        ...api,
        desktop: true,
        preferences: vi
          .fn()
          .mockResolvedValue({ revision: 0, instance_profiles: {} }),
        inspectInstallationRecognition: vi.fn().mockResolvedValue({
          installation_id: 'g',
          executable_sha256: 'hash',
          version: '0.I-1',
          recommended_profile: 'cdda-0i-1-macos-native-trial-v1',
          evidence: '匹配',
        }),
        instanceLaunchPlan,
        launch,
        saveInstanceProfile,
      }}
      data={{
        ...emptyDashboard,
        play_instances: [
          {
            id: 'i',
            installation_id: 'g',
            installation_sha256: 'hash',
            name: '自动实例',
            data_root: '/instance',
            binding_status: 'unverified',
            created_at: '',
          },
        ],
      }}
      onRefresh={vi.fn()}
    />,
  );
  await user.click(screen.getByRole('button', { name: '启动 自动实例' }));
  expect(await screen.findByRole('button', { name: '确认启动' })).toBeEnabled();
  expect(screen.queryByLabelText('游戏版本')).not.toBeInTheDocument();
  expect(screen.queryByLabelText('试运行参数方案')).not.toBeInTheDocument();
  expect(instanceLaunchPlan).toHaveBeenCalledWith(
    'i',
    'cdda-0i-1-macos-native-trial-v1',
  );
  expect(launch).not.toHaveBeenCalled();
  await user.click(screen.getByRole('button', { name: '取消' }));
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  expect(saveInstanceProfile).not.toHaveBeenCalled();
  expect(launch).not.toHaveBeenCalled();
});

it('keeps a saved custom profile until the user explicitly chooses automatic adaptation', async () => {
  const user = userEvent.setup();
  const instanceLaunchPlan = vi.fn();
  render(
    <PlayInstancesPanel
      api={{
        ...api,
        desktop: true,
        preferences: vi.fn().mockResolvedValue({
          revision: 1,
          instance_profiles: { i: 'cdda-0h-source-trial-v1' },
        }),
        inspectInstallationRecognition: vi.fn().mockResolvedValue({
          installation_id: 'g',
          executable_sha256: 'hash',
          version: '0.I-1',
          recommended_profile: 'cdda-0i-1-macos-native-trial-v1',
          evidence: '匹配',
        }),
        instanceLaunchPlan,
      }}
      data={{
        ...emptyDashboard,
        play_instances: [
          {
            id: 'i',
            installation_id: 'g',
            installation_sha256: 'hash',
            name: '保留设置',
            data_root: '/i',
            binding_status: 'unverified',
            created_at: '',
          },
        ],
      }}
      onRefresh={vi.fn()}
    />,
  );
  await user.click(screen.getByRole('button', { name: '启动设置 保留设置' }));
  expect(
    await screen.findByRole('button', { name: '使用自动适配' }),
  ).toBeEnabled();
  expect(instanceLaunchPlan).not.toHaveBeenCalled();
  await user.click(screen.getByRole('button', { name: '使用自动适配' }));
  expect(
    screen.getByRole('button', { name: '预览 保留设置 试运行' }),
  ).toBeEnabled();
  expect(instanceLaunchPlan).not.toHaveBeenCalled();
});

it.each([
  ['starting', '启动中'],
  ['running', '运行中'],
  ['interrupted_unknown', '待确认关闭'],
  ['exited_unverified', '启动'],
  ['resolved_unknown', '启动'],
])(
  'reflects a %s session in instance launch controls',
  async (state, label) => {
    const user = userEvent.setup();
    const launch = vi.fn();
    const instanceLaunchPlan = vi.fn();
    render(
      <PlayInstancesPanel
        api={{
          ...api,
          desktop: true,
          launch,
          instanceLaunchPlan,
          preferences: vi
            .fn()
            .mockResolvedValue({ revision: 0, instance_profiles: {} }),
        }}
        data={{
          ...emptyDashboard,
          play_instances: ['a', 'b'].map((id) => ({
            id,
            name: id.toUpperCase(),
            installation_id: 'g',
            installation_sha256: 'hash',
            data_root: '/' + id,
            binding_status: 'unverified',
            created_at: '',
          })),
          sessions: [
            {
              id: 'session',
              play_instance_id: 'a',
              installation_id: 'g',
              launch_profile_id: null,
              instance_data_root: '/a',
              state,
              pid: 42,
              started_at: '2026-10-29T00:00:00Z',
              finished_at: null,
              exit_code: null,
            },
          ],
        }}
        onRefresh={vi.fn()}
      />,
    );
    const button = screen.getByRole('button', { name: label + ' A' });
    const blocked = label !== '启动';
    if (blocked) expect(button).toBeDisabled();
    else expect(button).toBeEnabled();
    if (blocked) await user.click(button);
    expect(launch).not.toHaveBeenCalled();
    expect(instanceLaunchPlan).not.toHaveBeenCalled();
    expect(screen.getByRole('button', { name: '启动 B' })).toBeEnabled();
    if (state === 'interrupted_unknown') {
      expect(screen.getByText(/再到“任务与记录”页面/)).toBeInTheDocument();
    }
    await user.click(screen.getByRole('button', { name: '启动设置 A' }));
    await user.click(await screen.findByText('高级启动方案（旧版与实验选项）'));
    await user.selectOptions(
      screen.getByLabelText('试运行参数方案'),
      'cdda-0h-source-trial-v1',
    );
    await user.click(screen.getByRole('checkbox'));
    const preview = screen.getByRole('button', { name: '预览 A 试运行' });
    if (blocked) expect(preview).toBeDisabled();
    else expect(preview).toBeEnabled();
  },
);

it.each([null, 1, 0])(
  'surfaces abnormal home termination (%s) without claiming save health or relaunching',
  async (exit_code) => {
    const onOpenActivity = vi.fn();
    const launch = vi.fn();
    render(
      <PlayInstancesPanel
        home
        onOpenActivity={onOpenActivity}
        api={{ ...api, desktop: true, launch }}
        onRefresh={vi.fn()}
        data={{
          ...emptyDashboard,
          play_instances: [
            {
              id: 'i',
              installation_id: 'g',
              installation_sha256: 'hash',
              name: 'QA',
              data_root: '/qa',
              binding_status: 'unverified',
              created_at: '2026-10-30T00:00:00Z',
            },
          ],
          sessions: [
            {
              id: 's',
              installation_id: 'g',
              play_instance_id: 'i',
              launch_profile_id: null,
              instance_data_root: '/qa',
              pid: 123,
              started_at: '2026-10-30T00:00:00Z',
              finished_at: '2026-10-30T00:01:00Z',
              exit_code,
              state: 'exited_unverified',
            },
          ],
        }}
      />,
    );
    if (exit_code === 0) {
      expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    } else {
      expect(screen.getByRole('alert')).toHaveTextContent(
        '上次启动或运行需要检查',
      );
      expect(screen.getByRole('alert')).toHaveTextContent('不代表存档稳定');
      await userEvent
        .setup()
        .click(screen.getByRole('button', { name: '查看运行记录 QA' }));
      expect(onOpenActivity).toHaveBeenCalledOnce();
    }
    expect(launch).not.toHaveBeenCalled();
  },
);
