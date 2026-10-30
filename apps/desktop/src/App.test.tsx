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
  root_dir: null,
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
  environment: {},
  play_instance_id: null,
  launch_profile_id: null,
  instance_data_root: null,
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
    registerManagedInstance: vi.fn(),
    configureWorkspace: vi.fn(),
    openWorkspacePath: vi.fn(),
    workspaceDefaults: vi.fn().mockResolvedValue(null),
    preferences: vi
      .fn()
      .mockResolvedValue({ revision: 0, instance_profiles: {} }),
    saveInstanceProfile: vi.fn(),
    resetPreferences: vi.fn(),
    desktop: true,
    inspectContentConfig: vi.fn(),
    previewContentConfig: vi.fn(),
    prepareContentConfigProtection: vi.fn(),
    prepareConfigChange: vi.fn(),
    prepareConfigRollback: vi.fn(),
    applyConfigChange: vi.fn(),
    configChanges: vi.fn(),
    inspectConfigChange: vi.fn(),
    resolveConfigChange: vi.fn(),
    prepareInstanceContentInstall: vi.fn(),
    prepareContentInstall: vi.fn(),
    startContentInstall: vi.fn(),
    contentInstallTasks: vi.fn().mockResolvedValue([]),
    cancelContentInstall: vi.fn(),
    contentEntries: vi.fn().mockResolvedValue([]),
    registerContent: vi.fn(),
    refreshContent: vi.fn(),
    forgetContent: vi.fn(),
    prepareInstallationCopy: vi.fn(),
    startInstallationCopy: vi.fn(),
    cancelInstallationCopy: vi.fn(),
    registerCopiedInstallation: vi.fn(),
    dashboard: vi.fn().mockResolvedValue(data),
    pickPath: vi.fn(),
    initialize: vi.fn(),
    importInstallation: vi.fn(),
    registerPlayInstance: vi.fn(),
    instanceLaunchPlan: vi.fn(),
    previewInstanceResources: vi.fn(),
    launchPlan: vi.fn().mockResolvedValue(plan),
    launch: vi.fn(),
    acknowledge: vi.fn(),
    scanWorlds: vi.fn(),
    cancelScan: vi.fn(),
    previewRetention: vi.fn(),
    updateBackupDetails: vi.fn(),
    backupPlan: vi.fn(),
    instanceBackupPlan: vi.fn(),
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
    const api = client();
    render(<App api={api} />);
    expect(await screen.findByText('准备你的第一次游玩')).toBeVisible();
    await userEvent
      .setup()
      .click(screen.getByRole('button', { name: /导入已有游戏/ }));
    expect(await screen.findByText('数据存放位置')).toBeVisible();
    expect(
      screen.queryByRole('button', { name: /存档历史/ }),
    ).not.toBeInTheDocument();
    expect(api.importInstallation).not.toHaveBeenCalled();
    expect(api.launch).not.toHaveBeenCalled();
    expect(
      screen.getByRole('button', { name: /保存位置并继续/ }),
    ).toBeDisabled();
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
    await user.click(
      await screen.findByRole('button', { name: /导入已有游戏/ }),
    );
    await screen.findByText('数据存放位置');
    await user.click(screen.getByText('自定义子目录与路径操作'));
    for (const button of screen.getAllByRole('button', { name: '选择' }))
      await user.click(button);
    await user.click(screen.getByRole('button', { name: /保存位置并继续/ }));
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
    await user.click(await screen.findByRole('button', { name: /^游戏库/ }));
    await user.click(screen.getByRole('tab', { name: /安装版本/ }));
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
    await user.click(await screen.findByRole('button', { name: /^游戏库/ }));
    await user.click(screen.getByRole('tab', { name: /安装版本/ }));
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
    await user.click(
      await screen.findByRole('button', { name: /导入已有游戏/ }),
    );
    await screen.findByText('数据存放位置');
    await user.click(screen.getByText('自定义子目录与路径操作'));
    for (const button of screen.getAllByRole('button', { name: '选择' }))
      await user.click(button);
    await user.click(screen.getByRole('button', { name: /保存位置并继续/ }));
    expect(await screen.findByRole('alert')).toHaveTextContent('目录不能重叠');
    expect(screen.getByLabelText('存档目录')).toHaveValue('/same');
  });
  it('validates literal argument arrays without accepting shell strings', () => {
    expect(parseArguments('["$(touch marker)","with spaces"]')).toEqual([
      '$(touch marker)',
      'with spaces',
    ]);
    for (const bad of ['"--flag"', '{}', '[1]', '["\\u0000"]'])
      expect(() => parseArguments(bad)).toThrow();
  });
  it('explains signal termination without asserting Gatekeeper rejection', async () => {
    render(
      <App
        api={client({
          ...emptyDashboard,
          workspace,
          operations: [
            {
              id: 'signal',
              task_id: 'session',
              action: 'game.process_result',
              status: 'exited_unverified',
              error_code: 'PROCESS_TERMINATED_BY_SIGNAL',
              created_at: workspace.created_at,
            },
          ],
        })}
      />,
    );
    await userEvent
      .setup()
      .click(await screen.findByRole('button', { name: '任务与记录' }));
    const table = screen.getByRole('table');
    expect(
      within(table).getByText('进程被信号终止；原因需核对系统或游戏日志'),
    ).toBeVisible();
    expect(within(table).queryByText(/Gatekeeper/)).not.toBeInTheDocument();
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
      .click(await screen.findByRole('button', { name: '任务与记录' }));
    const table = screen.getByRole('table');
    expect(within(table).queryByText('启动失败')).not.toBeInTheDocument();
    expect(within(table).getByText('失败')).toBeVisible();
    expect(within(table).getByText('检查已恢复目录')).toBeVisible();
  });
  it.each(['cdda-0i-1-macos-trial-v2', 'cdda-0i-1-macos-native-trial-v1'])(
    'requires acknowledgement and previews environment for %s',
    async (profileId) => {
      const api = client({
        ...emptyDashboard,
        workspace,
        installations: [game],
        play_instances: [
          {
            id: 'instance',
            installation_id: game.id,
            installation_sha256: game.sha256,
            name: '测试实例',
            data_root: '/instance',
            binding_status: 'unverified',
            created_at: workspace.created_at,
          },
        ],
      });
      vi.mocked(api.instanceLaunchPlan).mockResolvedValue({
        ...plan,
        play_instance_id: 'instance',
        launch_profile_id: profileId,
        instance_data_root: '/instance',
        environment: {
          HOME: '/instance/.hub-runtime/home',
          TMPDIR: '/instance/.hub-runtime/tmp',
        },
      });
      const user = userEvent.setup();
      render(<App api={api} />);
      await user.click(await screen.findByRole('button', { name: /^游戏库/ }));
      await user.click(screen.getByRole('button', { name: '启动 测试实例' }));
      await screen.findByLabelText('试运行参数方案');
      await user.selectOptions(
        screen.getByLabelText('试运行参数方案'),
        'cdda-0h-source-trial-v1',
      );
      const ack = screen.getByRole('checkbox', {
        name: /使用所选源码参数方案/,
      });
      await user.click(ack);
      await user.selectOptions(
        screen.getByLabelText('试运行参数方案'),
        profileId,
      );
      expect(ack).not.toBeChecked();
      const preview = screen.getByRole('button', {
        name: '预览 测试实例 试运行',
      });
      expect(preview).toBeDisabled();
      await user.click(ack);
      await user.click(preview);
      await waitFor(() =>
        expect(api.instanceLaunchPlan).toHaveBeenCalledWith(
          'instance',
          profileId,
        ),
      );
      const dialog = await screen.findByRole('dialog');
      await user.click(within(dialog).getByText('启动技术详情'));
      expect(
        within(dialog).getByText(/"HOME":"\/instance\/.hub-runtime\/home"/),
      ).toBeVisible();
      expect(api.launch).not.toHaveBeenCalled();
      await user.click(within(dialog).getByRole('button', { name: '取消' }));
      expect(api.launch).not.toHaveBeenCalled();
    },
  );
});

it('opens the worlds subview from a scan task in the task center', async () => {
  const service = client({
    ...emptyDashboard,
    workspace,
    scan_tasks: [
      {
        id: 'scan-route',
        request_key: 'scan-key',
        workspace_id: workspace.id,
        operation: 'world.scan',
        state: 'failed',
        phase: 'finished',
        processed_entries: 0,
        revision: 1,
        error_code: 'READ_FAILED',
        created_at: workspace.created_at,
        finished_at: workspace.created_at,
      },
    ],
  });
  const user = userEvent.setup();
  render(<App api={service} />);
  await user.click(await screen.findByRole('button', { name: '任务与记录' }));
  await user.click(screen.getByRole('button', { name: '打开世界扫描管理' }));
  expect(screen.getByRole('tab', { name: '世界与角色' })).toHaveAttribute(
    'aria-selected',
    'true',
  );
  expect(service.scanWorlds).not.toHaveBeenCalled();
  expect(service.launch).not.toHaveBeenCalled();
});

it('searches installations and opens only the selected version instances without launching', async () => {
  const otherGame = {
    ...game,
    id: 'other',
    name: 'CDDA alternate',
    version: '0.I',
  };
  const instance = {
    id: 'instance-a',
    installation_id: game.id,
    installation_sha256: game.sha256,
    name: 'World A',
    data_root: '/a',
    binding_status: 'unverified',
    created_at: workspace.created_at,
  };
  const service = client({
    ...emptyDashboard,
    workspace,
    installations: [game, otherGame],
    play_instances: [
      instance,
      {
        ...instance,
        id: 'instance-b',
        installation_id: otherGame.id,
        name: 'World B',
        data_root: '/b',
      },
    ],
  });
  const user = userEvent.setup();
  render(<App api={service} />);
  await user.click(await screen.findByRole('button', { name: /^游戏库/ }));
  await user.click(screen.getByRole('tab', { name: /安装版本/ }));
  await user.type(screen.getByLabelText('搜索安装版本'), '0.I');
  expect(
    screen.queryByRole('button', { name: '查看 本地 CDDA 关联详情' }),
  ).not.toBeInTheDocument();
  await user.click(
    screen.getByRole('button', { name: '查看 CDDA alternate 关联详情' }),
  );
  await user.click(screen.getByRole('button', { name: '管理此版本的实例' }));
  expect(screen.getByLabelText('筛选安装版本')).toHaveValue('other');
  expect(
    screen.getByRole('heading', { name: 'World B · 绑定未验证' }),
  ).toBeVisible();
  expect(
    screen.queryByRole('heading', { name: 'World A · 绑定未验证' }),
  ).not.toBeInTheDocument();
  await user.selectOptions(screen.getByLabelText('筛选安装版本'), '');
  await user.type(screen.getByLabelText('搜索游玩实例'), '/a');
  expect(
    screen.getByRole('heading', { name: 'World A · 绑定未验证' }),
  ).toBeVisible();
  expect(
    screen.queryByRole('heading', { name: 'World B · 绑定未验证' }),
  ).not.toBeInTheDocument();
  expect(service.launch).not.toHaveBeenCalled();
  expect(service.instanceLaunchPlan).not.toHaveBeenCalled();
  expect(service.registerPlayInstance).not.toHaveBeenCalled();
});

it('does not hide a live session behind an earlier returned exit record', async () => {
  const session = {
    id: 'recent',
    installation_id: game.id,
    play_instance_id: null,
    launch_profile_id: null,
    instance_data_root: null,
    state: 'exited_unverified',
    pid: 1,
    started_at: '2026-10-29T02:00:00Z',
    finished_at: '2026-10-29T03:00:00Z',
    exit_code: 0,
  };
  const service = client({
    ...emptyDashboard,
    workspace,
    installations: [game],
    sessions: [
      session,
      {
        ...session,
        id: 'older-live',
        state: 'running',
        started_at: '2026-10-29T01:00:00Z',
        finished_at: null,
      },
    ],
  });
  const user = userEvent.setup();
  render(<App api={service} />);
  await user.click(await screen.findByRole('button', { name: /^游戏库/ }));
  await user.click(screen.getByRole('tab', { name: /安装版本/ }));
  const card = screen.getByRole('article');
  expect(within(card).getByText('运行中')).toBeVisible();
  expect(
    within(card).getByRole('button', { name: /工作区忙碌或待确认/ }),
  ).toBeDisabled();
});

it('offers platform defaults without requiring three directory picker operations', async () => {
  const service = client();
  const defaults = {
    save_dir: '/system/app.workspace/saves',
    backup_dir: '/system/app.workspace/backups',
    content_dir: '/system/app.workspace/content',
  };
  vi.mocked(service.workspaceDefaults).mockResolvedValue(defaults);
  vi.mocked(service.initialize).mockResolvedValue({
    ...workspace,
    ...defaults,
  });
  const user = userEvent.setup();
  render(<App api={service} />);
  await user.click(await screen.findByRole('button', { name: /导入已有游戏/ }));
  await screen.findByRole('button', { name: '使用系统推荐目录' });
  await user.click(screen.getByText('自定义子目录与路径操作'));
  await waitFor(() =>
    expect(screen.getByLabelText('存档目录')).toHaveValue(defaults.save_dir),
  );
  await user.click(screen.getByRole('button', { name: /保存位置并继续/ }));
  expect(service.configureWorkspace).toHaveBeenCalledWith(
    '/system/app.workspace',
    defaults,
    null,
  );
  expect(service.pickPath).not.toHaveBeenCalled();
});

it('continues local import into independent environment creation without launching', async () => {
  let data: Dashboard = { ...emptyDashboard, workspace };
  const service = client();
  vi.mocked(service.dashboard).mockImplementation(async () => data);
  vi.mocked(service.pickPath).mockResolvedValue(game.executable);
  vi.mocked(service.importInstallation).mockImplementation(async () => {
    data = { ...data, installations: [game] };
    return game;
  });
  const instance = {
    id: 'first',
    installation_id: game.id,
    installation_sha256: game.sha256,
    name: '主力生存',
    data_root: '/independent/first',
    binding_status: 'unverified',
    created_at: '2026-10-30T00:00:00Z',
  };
  vi.mocked(service.registerManagedInstance).mockImplementation(async () => {
    data = { ...data, play_instances: [instance] };
    return instance;
  });
  const u = userEvent.setup();
  render(<App api={service} />);
  await u.click(await screen.findByRole('button', { name: /导入已有游戏/ }));
  await u.click(screen.getByRole('button', { name: '选择本地游戏' }));
  const dialog = screen.getByRole('dialog');
  await u.type(within(dialog).getByLabelText('安装名称'), '本地 CDDA');
  await u.click(within(dialog).getByRole('button', { name: '选择' }));
  await u.click(within(dialog).getByRole('button', { name: '导入安装' }));
  await screen.findByLabelText('游玩环境名称');
  expect(service.importInstallation).toHaveBeenCalledWith(
    game.executable,
    '本地 CDDA',
  );
  expect(service.registerManagedInstance).not.toHaveBeenCalled();
  await u.type(screen.getByLabelText('游玩环境名称'), '主力生存');
  await u.click(screen.getByRole('button', { name: '创建独立游玩环境' }));
  await screen.findByText('环境已创建');
  expect(
    screen.queryByText('游戏已导入，接下来创建独立游玩环境。'),
  ).not.toBeInTheDocument();
  expect(service.registerManagedInstance).toHaveBeenCalledExactlyOnceWith(
    game.id,
    '主力生存',
  );
  expect(service.launch).not.toHaveBeenCalled();
  await u.click(screen.getByRole('button', { name: '前往启动' }));
  expect(
    await screen.findByRole('button', { name: '启动 主力生存' }),
  ).toBeEnabled();
  expect(service.launch).not.toHaveBeenCalled();
  expect(screen.getByRole('status')).toHaveTextContent('游玩环境已创建');
  expect(service.registerPlayInstance).not.toHaveBeenCalled();
  await u.click(screen.getByRole('button', { name: '添加游玩环境' }));
  expect(screen.queryByRole('status')).not.toBeInTheDocument();
});

it('keeps first-play progress when visiting settings and gives both directory forms independent labels', async () => {
  const service = client();
  const u = userEvent.setup();
  render(<App api={service} />);
  await u.click(await screen.findByRole('button', { name: /导入已有游戏/ }));
  await screen.findByText('数据存放位置');
  await u.click(screen.getByRole('button', { name: '设置' }));
  expect(screen.getByRole('textbox', { name: '数据根目录' })).toBeVisible();
  await u.click(screen.getByRole('button', { name: '游玩环境' }));
  expect(screen.getByText('数据存放位置')).toBeVisible();
  expect(screen.getByRole('list', { name: '创建进度' })).toBeVisible();
  expect(service.initialize).not.toHaveBeenCalled();
  expect(service.registerManagedInstance).not.toHaveBeenCalled();
});
