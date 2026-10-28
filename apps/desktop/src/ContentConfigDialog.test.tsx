import { render, screen } from '@testing-library/react';
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
it('distinguishes missing and broken files from explicit empty configuration and catalog absence', () => {
  render(<ContentConfigDialog report={report} onClose={vi.fn()} />);
  expect(
    screen.getByText(/文件或目录缺失，不能推断默认值/),
  ).toBeInTheDocument();
  expect(screen.getByText(/JSON 损坏或正在变化/)).toBeInTheDocument();
  expect(screen.getByText('声明顺序：显式空列表')).toBeInTheDocument();
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
it('shows ambiguous registrations without selecting a winner', () => {
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
  await user.click(
    screen.getByRole('button', { name: '读取 A 模组与声音引用' }),
  );
  expect(inspectContentConfig).toHaveBeenCalledWith('p');
  expect(instanceLaunchPlan).not.toHaveBeenCalled();
  expect(await screen.findByRole('dialog')).toBeInTheDocument();
});

it('keeps incomplete discovery and dependency ambiguity distinct from compatibility', () => {
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

it('previews sound enablement with unknown baseline without inventing default values', async () => {
  const user = userEvent.setup();
  render(<ContentConfigDialog report={report} onClose={vi.fn()} />);
  await user.selectOptions(screen.getByLabelText('声音开关草稿'), 'true');
  await user.click(screen.getByRole('button', { name: '比较配置草稿' }));
  expect(screen.getByLabelText('配置草稿结果')).toHaveTextContent(
    '原声明：未知',
  );
  expect(screen.getByLabelText('配置草稿结果')).toHaveTextContent(
    '草稿："true"',
  );
  expect(
    screen.queryByRole('button', { name: /应用|确认写入/ }),
  ).not.toBeInTheDocument();
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
              },
              {
                content_id: 'unknown',
                name: 'Unknown',
                dependencies: null,
                version: null,
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
