import ContentConfigProtectionPanel from './ContentConfigProtectionPanel';
import type { ContentConfigProtectionPlan } from './bindings/ContentConfigProtectionPlan';
import type { BackupTask } from './api';
import type { ContentConfigPlan } from './bindings/ContentConfigPlan';
import { act, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { it, expect, vi } from 'vitest';
import ContentConfigDialog from './ContentConfigDialog';
import { previewContentConfig } from './contentConfigPreview';
import PlayInstancesPanel from './PlayInstancesPanel';
import { api, emptyDashboard } from './api';
import type { ContentConfigReport } from './bindings/ContentConfigReport';
const report: ContentConfigReport = {
  discovery: { scopes: [], packages: [], findings: [], truncated: false },
  instance_id: 'p',
  data_root: '/instance',
  installation_version: null,
  checked_at: 'now',
  sound: {
    path: 'config/options.json',
    status: 'missing',
    sha256: null,
    soundpack: null,
    sound_enabled: null,
  },
  default_mods: {
    path: 'config/user-default-mods.json',
    status: 'read',
    sha256: 'a',
    mod_ids: ['dda'],
  },
  worlds: [
    { path: 'save/A/mods.json', status: 'read', sha256: 'b', mod_ids: [] },
    {
      path: 'save/B/mods.json',
      status: 'invalid_json',
      sha256: null,
      mod_ids: [],
    },
  ],
  references: [
    {
      kind: 'mod',
      content_id: 'dda',
      referenced_by: ['config/user-default-mods.json'],
      registered_roots: [],
    },
  ],
  issues: [],
  binding_verified: false,
};
it('distinguishes missing and broken files from explicit empty configuration and catalog absence', async () => {
  render(<ContentConfigDialog report={report} onClose={vi.fn()} />);
  expect(
    screen.getByText(/文件或目录缺失，不能推断默认值/),
  ).toBeInTheDocument();
  expect(screen.getByText(/JSON 损坏或正在变化/)).toBeInTheDocument();
  expect(screen.getByText('声明顺序：显式空列表')).toBeInTheDocument();
  await userEvent.setup().click(screen.getByRole('tab', { name: /资源诊断/ }));
  expect(
    screen.getByText(/资源库未登记匹配项，不代表目标游戏缺少该资源/),
  ).toBeInTheDocument();
  expect(
    screen.getByText(/此列表不代表已有世界的模组组合/),
  ).toBeInTheDocument();
  expect(
    screen.queryByRole('button', { name: /启用|写入|卸载/ }),
  ).not.toBeInTheDocument();
});
it('shows ambiguous registrations without selecting a winner', async () => {
  render(
    <ContentConfigDialog
      report={{
        ...report,
        references: [
          {
            kind: 'mod',
            content_id: 'dda',
            referenced_by: ['save/A/mods.json'],
            registered_roots: ['/one', '/two'],
          },
        ],
      }}
      onClose={vi.fn()}
    />,
  );
  await userEvent.setup().click(screen.getByRole('tab', { name: /资源诊断/ }));
  expect(screen.getByText(/存在歧义，不能自动选择/)).toBeInTheDocument();
});
it('opens read-only inspection for an instance without creating a launch plan', async () => {
  const inspectContentConfig = vi.fn().mockResolvedValue(report);
  const instanceLaunchPlan = vi.fn();
  const user = userEvent.setup();
  render(
    <PlayInstancesPanel
      api={{ ...api, desktop: true, inspectContentConfig, instanceLaunchPlan }}
      data={{
        ...emptyDashboard,
        play_instances: [
          {
            id: 'p',
            installation_id: 'i',
            installation_sha256: 'hash',
            name: 'A',
            data_root: '/instance',
            binding_status: 'unverified',
            created_at: 'now',
          },
        ],
      }}
      onRefresh={vi.fn()}
    />,
  );
  await user.click(screen.getByRole('button', { name: '配置 A' }));
  expect(inspectContentConfig).toHaveBeenCalledWith('p');
  expect(instanceLaunchPlan).not.toHaveBeenCalled();
  expect(await screen.findByRole('dialog')).toBeInTheDocument();
});

it('keeps incomplete discovery and dependency ambiguity distinct from compatibility', async () => {
  render(
    <ContentConfigDialog
      report={{
        ...report,
        discovery: {
          scopes: [
            {
              path: '/instance/mods',
              kind: 'mod',
              basis: 'instance_userdir',
              status: 'partial',
            },
          ],
          packages: [],
          truncated: true,
          findings: [
            {
              code: 'duplicate_id',
              context: 'mod',
              content_id: 'dda',
              related_id: null,
            },
            {
              code: 'dependency_not_observed',
              context: '/instance/mods/test',
              content_id: 'test',
              related_id: 'missing',
            },
          ],
        },
      }}
      onClose={vi.fn()}
    />,
  );
  await userEvent.setup().click(screen.getByRole('tab', { name: /资源诊断/ }));
  expect(screen.getByRole('alert')).toHaveTextContent('候选扫描不完整');
  expect(screen.getByText(/重复 ID，未选择覆盖赢家/)).toBeInTheDocument();
  expect(screen.getByText(/依赖未在本次候选范围中发现/)).toBeInTheDocument();
  expect(
    screen.getByText(/不证明实际加载、版本或存档兼容/),
  ).toBeInTheDocument();
  expect(
    screen.queryByRole('button', { name: /启用|写入|卸载/ }),
  ).not.toBeInTheDocument();
});

it('previews an explicit empty default without changing existing worlds and invalidates on edits', async () => {
  const user = userEvent.setup();
  render(<ContentConfigDialog report={report} onClose={vi.fn()} />);
  await userEvent.setup().click(screen.getByRole('tab', { name: '修改配置' }));
  await user.click(screen.getByLabelText('拟调整新世界默认模组'));
  await user.clear(screen.getByRole('textbox', { name: /默认模组 ID/ }));
  await user.click(screen.getByRole('button', { name: '比较配置草稿' }));
  const result = screen.getByLabelText('配置草稿结果');
  expect(result).toHaveTextContent('原声明：["dda"]');
  expect(result).toHaveTextContent('草稿：[]');
  expect(result).not.toHaveTextContent('save/A/mods.json');
  expect(result).toHaveTextContent('本预览未创建或检查保护点');
  await user.type(screen.getByRole('textbox', { name: /默认模组 ID/ }), 'new');
  expect(screen.queryByLabelText('配置草稿结果')).not.toBeInTheDocument();
  expect(report.default_mods.mod_ids).toEqual(['dda']);
});

it('keeps unknown sound baselines explicit in diagnostic drafts', () => {
  const result = previewContentConfig(report, {
    defaultMods: null,
    soundpack: null,
    soundEnabled: 'true',
  });
  expect(result.differences[0]).toMatchObject({
    before: null,
    after: 'true',
    baselineKnown: false,
  });
  expect(result.issues.join(' ')).toContain('原值未知');
});

it('preserves no-change intent and reports order-only changes without altering source data', () => {
  expect(
    previewContentConfig(report, {
      defaultMods: null,
      soundpack: null,
      soundEnabled: null,
    }).differences,
  ).toEqual([]);
  const source = {
    ...report,
    default_mods: { ...report.default_mods, mod_ids: ['a', 'b'] },
  };
  const result = previewContentConfig(source, {
    defaultMods: ['b', 'a'],
    soundpack: null,
    soundEnabled: null,
  });
  expect(result.differences[0]).toMatchObject({
    before: ['a', 'b'],
    after: ['b', 'a'],
    baselineKnown: true,
  });
  expect(source.default_mods.mod_ids).toEqual(['a', 'b']);
});

it('rejects duplicate and oversized IDs without silently normalizing the requested combination', () => {
  for (const ids of [
    ['a', 'a'],
    ['界'.repeat(86)],
    ['bad\u0000id'],
    Array.from({ length: 257 }, (_, i) => `id${i}`),
  ]) {
    const result = previewContentConfig(report, {
      defaultMods: ids,
      soundpack: null,
      soundEnabled: null,
    });
    expect(result.differences).toEqual([]);
    expect(result.issues.length).toBeGreaterThan(0);
  }
});

it('checks proposed dependencies against candidate definitions rather than catalog registration', () => {
  const result = previewContentConfig(
    {
      ...report,
      discovery: {
        scopes: [
          {
            path: '/mods',
            kind: 'mod',
            basis: 'instance_userdir',
            status: 'partial',
          },
        ],
        truncated: false,
        packages: [
          {
            path: '/mods/local',
            kind: 'mod',
            metadata_sha256: 'hash',
            definitions: [
              {
                content_id: 'local',
                name: 'Local',
                dependencies: ['dda'],
                version: null,
                obsolete: false,
              },
              {
                content_id: 'unknown',
                name: 'Unknown',
                dependencies: null,
                version: null,
                obsolete: false,
              },
            ],
          },
        ],
        findings: [
          {
            code: 'dependency_cycle',
            context: 'candidate_mods',
            content_id: 'local',
            related_id: 'local',
          },
        ],
      },
    },
    {
      defaultMods: ['local', 'unknown', 'external'],
      soundpack: 'music',
      soundEnabled: null,
    },
  );
  expect(result.issues.join('\n')).toContain('草稿未列出声明依赖 dda');
  expect(result.issues.join('\n')).toContain('依赖声明未知');
  expect(result.issues.join('\n')).toContain('循环依赖');
  expect(result.issues.join('\n')).toContain('external：本次候选范围未发现');
  expect(result.issues.join('\n')).toContain(
    '声音包 music：本次候选范围未发现',
  );
  expect(result.issues.join('\n')).toContain('候选扫描不完整');
});

it('does not choose duplicate candidate sources and bounds large dependency diagnostics', () => {
  const definition = {
    content_id: 'duplicate',
    name: 'Duplicate',
    dependencies: [],
    version: null,
    obsolete: false,
  };
  const duplicateReport = {
    ...report,
    discovery: {
      ...report.discovery,
      packages: [
        {
          path: '/one',
          kind: 'mod',
          metadata_sha256: 'a',
          definitions: [definition],
        },
        {
          path: '/two',
          kind: 'mod',
          metadata_sha256: 'b',
          definitions: [definition],
        },
      ],
    },
  };
  expect(
    previewContentConfig(duplicateReport, {
      defaultMods: ['duplicate'],
      soundpack: null,
      soundEnabled: null,
    }).issues.join(''),
  ).toContain('候选重复，未选择覆盖来源');
  const ids = Array.from({ length: 256 }, (_, i) => `id${i}`);
  const many = {
    ...report,
    discovery: {
      ...report.discovery,
      packages: [
        {
          path: '/many',
          kind: 'mod',
          metadata_sha256: 'c',
          definitions: ids.map((id) => ({
            content_id: id,
            name: id,
            version: null,
            obsolete: false,
            dependencies: ['missing1', 'missing2'],
          })),
        },
      ],
    },
  };
  const result = previewContentConfig(many, {
    defaultMods: ids,
    soundpack: null,
    soundEnabled: null,
  });
  expect(result.issues).toHaveLength(257);
  expect(result.issues.at(-1)).toContain('超过展示上限');
});

it('only requests a fresh backend preview on explicit action and shows its new baseline', async () => {
  const user = userEvent.setup();
  const plan: ContentConfigPlan = {
    report: { ...report, checked_at: 'fresh-read' },
    files: [
      {
        path: 'config/options.json',
        before_sha256: 'original-hash',
        proposed_sha256: 'new-hash',
        proposed_json: '[]',
      },
    ],
    proposed_discovery: report.discovery,
    write_supported: false,
  };
  const previewContentConfig = vi.fn().mockResolvedValue(plan);
  render(
    <ContentConfigDialog
      report={{ ...report, sound: { ...report.sound, status: 'read' } }}
      api={{ previewContentConfig }}
      onClose={vi.fn()}
    />,
  );
  await userEvent.setup().click(screen.getByRole('tab', { name: '修改配置' }));
  expect(previewContentConfig).not.toHaveBeenCalled();
  await user.selectOptions(screen.getByLabelText('声音开关草稿'), 'true');
  await user.click(
    screen.getByRole('button', { name: '重新读取并生成后端预览' }),
  );
  expect(previewContentConfig).toHaveBeenCalledExactlyOnceWith('p', {
    default_mods: null,
    soundpack: null,
    sound_enabled: true,
  });
  expect(await screen.findByLabelText('后端配置预览')).toHaveTextContent(
    'fresh-read',
  );
  expect(screen.getByLabelText('后端配置预览')).toHaveTextContent(
    'original-hash',
  );
  expect(
    screen.queryByRole('button', { name: /确认写入|应用草稿/ }),
  ).not.toBeInTheDocument();
});

it('discards an in-flight backend preview after editing the draft', async () => {
  const user = userEvent.setup();
  let complete: (plan: ContentConfigPlan) => void = () => {};
  const pending = new Promise<ContentConfigPlan>((resolve) => {
    complete = resolve;
  });
  const previewContentConfig = vi.fn().mockReturnValue(pending);
  render(
    <ContentConfigDialog
      report={{ ...report, sound: { ...report.sound, status: 'read' } }}
      api={{ previewContentConfig }}
      onClose={vi.fn()}
    />,
  );
  await userEvent.setup().click(screen.getByRole('tab', { name: '修改配置' }));
  await user.click(
    screen.getByRole('button', { name: '重新读取并生成后端预览' }),
  );
  await user.selectOptions(screen.getByLabelText('声音开关草稿'), 'false');
  await act(async () => {
    complete({
      report,
      files: [],
      proposed_discovery: report.discovery,
      write_supported: false,
    });
    await pending;
  });
  expect(screen.queryByLabelText('后端配置预览')).not.toBeInTheDocument();
  expect(
    screen.getByRole('button', { name: '重新读取并生成后端预览' }),
  ).toBeEnabled();
});

it('shows the backend refusal reason without keeping a successful preview', async () => {
  const user = userEvent.setup();
  const previewContentConfig = vi.fn().mockRejectedValue({
    code: 'CONTENT_CONFIG_PREVIEW_UNAVAILABLE',
    message: '目标声音字段尚未声明，不推断版本默认格式。',
  });
  render(
    <ContentConfigDialog
      report={report}
      api={{ previewContentConfig }}
      onClose={vi.fn()}
    />,
  );
  await userEvent.setup().click(screen.getByRole('tab', { name: '修改配置' }));
  await user.click(
    screen.getByRole('button', { name: '重新读取并生成后端预览' }),
  );
  expect(
    await screen.findByText('目标声音字段尚未声明，不推断版本默认格式。'),
  ).toBeInTheDocument();
  expect(screen.queryByLabelText('后端配置预览')).not.toBeInTheDocument();
});

const protection: ContentConfigProtectionPlan = {
  configuration: {
    report,
    write_supported: false,
    proposed_discovery: report.discovery,
    files: [
      {
        path: 'config/options.json',
        before_sha256: 'original',
        proposed_sha256: 'after',
        proposed_json: '[]',
      },
    ],
  },
  backup: {
    id: 'backup-plan',
    plan_digest: 'digest',
    play_instance_id: 'p',
    source_root: '/instance',
    backup_dir: '/backups',
    entry_count: 2,
    file_count: 2,
    total_bytes: '123',
    required_free_bytes: '456',
    consistency: 'best_effort',
    expires_at: 'later',
    entries: [],
    warnings: ['尽力快照'],
  },
};
const protectionTask: BackupTask = {
  id: 'protection-task',
  request_key: 'key',
  workspace_id: 'w',
  backup_id: 'backup',
  operation: 'backup.create',
  state: 'running',
  phase: 'copying',
  processed_entries: 0,
  revision: 1,
  error_code: null,
  created_at: 'now',
  finished_at: null,
};
it('requires scope acknowledgement before protection backup and displays refreshed terminal status', async () => {
  const user = userEvent.setup();
  const prepareContentConfigProtection = vi.fn().mockResolvedValue(protection);
  const createBackup = vi.fn().mockResolvedValue(protectionTask);
  const dashboard = vi.fn().mockResolvedValue({
    ...emptyDashboard,
    backup_tasks: [
      { ...protectionTask, state: 'succeeded', phase: 'finished' },
    ],
  });
  render(
    <ContentConfigProtectionPanel
      api={{
        prepareContentConfigProtection,
        createBackup,
        dashboard,
        cancelBackup: vi.fn(),
      }}
      instanceId="p"
      input={{ default_mods: null, soundpack: null, sound_enabled: true }}
    />,
  );
  expect(prepareContentConfigProtection).not.toHaveBeenCalled();
  await user.click(screen.getByRole('button', { name: '预览配置保护备份' }));
  expect(prepareContentConfigProtection).toHaveBeenCalledExactlyOnceWith('p', {
    default_mods: null,
    soundpack: null,
    sound_enabled: true,
  });
  expect(createBackup).not.toHaveBeenCalled();
  expect(screen.getByLabelText('配置保护备份范围')).toHaveTextContent(
    '/instance',
  );
  expect(
    screen.getByRole('button', { name: '确认创建保护备份' }),
  ).toBeDisabled();
  await user.click(screen.getByLabelText(/我确认备份整个实例/));
  await user.click(screen.getByRole('button', { name: '确认创建保护备份' }));
  expect(createBackup).toHaveBeenCalledWith(
    'backup-plan',
    'digest',
    expect.any(String),
  );
  expect(screen.getByText(/备份执行中/)).toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: '刷新保护备份状态' }));
  expect(screen.getByText(/备份任务已完成/)).toBeInTheDocument();
  expect(screen.getByText(/未执行配置写入/)).toBeInTheDocument();
  expect(
    screen.queryByRole('button', { name: '取消保护备份' }),
  ).not.toBeInTheDocument();
});

it('reuses the same request key after an ambiguous backup response and permits cancellation', async () => {
  const user = userEvent.setup();
  const createBackup = vi
    .fn()
    .mockRejectedValueOnce(new Error('连接中断'))
    .mockResolvedValue(protectionTask);
  const cancelBackup = vi
    .fn()
    .mockResolvedValue({ ...protectionTask, state: 'cancelling' });
  render(
    <ContentConfigProtectionPanel
      api={{
        prepareContentConfigProtection: vi.fn().mockResolvedValue(protection),
        createBackup,
        dashboard: vi.fn(),
        cancelBackup,
      }}
      instanceId="p"
      input={{ default_mods: null, soundpack: null, sound_enabled: true }}
    />,
  );
  await user.click(screen.getByRole('button', { name: '预览配置保护备份' }));
  await user.click(screen.getByLabelText(/我确认备份整个实例/));
  await user.click(screen.getByRole('button', { name: '确认创建保护备份' }));
  expect(await screen.findByText('连接中断')).toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: '确认创建保护备份' }));
  expect(createBackup.mock.calls[1]).toEqual(createBackup.mock.calls[0]);
  await user.click(screen.getByRole('button', { name: '取消保护备份' }));
  expect(cancelBackup).toHaveBeenCalledWith('protection-task');
  expect(screen.getByText(/正在取消/)).toBeInTheDocument();
});

it('selects discovered resources for this instance and preserves explicit mod order', async () => {
  const user = userEvent.setup();
  const definition = (
    content_id: string,
    name: string,
    dependencies: string[] = [],
  ) => ({ content_id, name, dependencies, version: null, obsolete: false });
  render(
    <ContentConfigDialog
      report={{
        ...report,
        sound: { ...report.sound, status: 'read' },
        discovery: {
          ...report.discovery,
          packages: [
            {
              path: '/game/data/mods/dda',
              kind: 'mod',
              metadata_sha256: 'a',
              definitions: [definition('dda', '基础内容')],
            },
            {
              path: '/instance/mods/extra',
              kind: 'mod',
              metadata_sha256: 'b',
              definitions: [definition('extra', '扩展内容', ['dda'])],
            },
            {
              path: '/instance/sound/pack',
              kind: 'soundpack',
              metadata_sha256: 'c',
              definitions: [definition('pack', '环境声音')],
            },
            {
              path: '/game/data/mods/dup1',
              kind: 'mod',
              metadata_sha256: 'd',
              definitions: [definition('dup', '重复项')],
            },
            {
              path: '/instance/mods/dup2',
              kind: 'mod',
              metadata_sha256: 'e',
              definitions: [definition('dup', '重复项')],
            },
          ],
        },
      }}
      onClose={vi.fn()}
    />,
  );
  await user.click(screen.getByRole('tab', { name: '修改配置' }));
  await user.click(screen.getByLabelText('拟调整新世界默认模组'));
  expect(
    screen.getByRole('option', { name: /重复项.*重复 ID/ }),
  ).toBeDisabled();
  await user.selectOptions(screen.getByLabelText('添加可用模组'), 'extra');
  await user.click(screen.getByRole('button', { name: '添加模组' }));
  expect(screen.getByRole('textbox', { name: /默认模组 ID/ })).toHaveValue(
    'dda\nextra',
  );
  await user.click(screen.getByRole('button', { name: '上移 extra' }));
  expect(screen.getByRole('textbox', { name: /默认模组 ID/ })).toHaveValue(
    'extra\ndda',
  );
  await user.click(screen.getByLabelText('拟调整声音包'));
  await user.selectOptions(screen.getByLabelText('选择已发现声音包'), 'pack');
  expect(screen.getByRole('textbox', { name: '目标声音包 ID' })).toHaveValue(
    'pack',
  );
  await user.click(screen.getByRole('button', { name: '比较配置草稿' }));
  expect(screen.getByLabelText('配置草稿结果')).toHaveTextContent('extra');
  await user.click(screen.getByRole('button', { name: '移除 extra' }));
  expect(screen.queryByLabelText('配置草稿结果')).not.toBeInTheDocument();
  expect(report.default_mods.mod_ids).toEqual(['dda']);
});

it('disables missing-file edits and explains game initialization', async () => {
  render(
    <ContentConfigDialog
      report={{
        ...report,
        default_mods: { ...report.default_mods, status: 'missing' },
      }}
      onClose={vi.fn()}
    />,
  );
  await userEvent.setup().click(screen.getByRole('tab', { name: '修改配置' }));
  expect(
    screen.getByRole('checkbox', { name: '拟调整新世界默认模组' }),
  ).toBeDisabled();
  expect(screen.getByRole('checkbox', { name: '拟调整声音包' })).toBeDisabled();
  expect(screen.getByRole('combobox', { name: '声音开关草稿' })).toBeDisabled();
  expect(
    screen.getByText(/请先启动游戏，在游戏中保存默认模组设置/),
  ).toBeInTheDocument();
});

it('keeps obsolete mods visible but disables adding them to new-world defaults', async () => {
  render(
    <ContentConfigDialog
      report={{
        ...report,
        discovery: {
          ...report.discovery,
          packages: [
            {
              path: '/mods/old',
              kind: 'mod',
              metadata_sha256: 'old',
              definitions: [
                {
                  content_id: 'old',
                  name: '旧模组',
                  dependencies: [],
                  version: null,
                  obsolete: true,
                },
              ],
            },
          ],
        },
      }}
      onClose={vi.fn()}
    />,
  );
  const u = userEvent.setup();
  await u.click(screen.getByRole('tab', { name: '修改配置' }));
  await u.click(screen.getByRole('checkbox', { name: '拟调整新世界默认模组' }));
  expect(screen.getByRole('option', { name: /旧模组.*过时/ })).toBeDisabled();
});
