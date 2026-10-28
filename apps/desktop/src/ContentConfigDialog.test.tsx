import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { it, expect, vi } from 'vitest';
import ContentConfigDialog from './ContentConfigDialog';
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
