import { act, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { expect, it, vi } from 'vitest';
import InstanceContentPanel from './InstanceContentPanel';
import { api, emptyDashboard } from './api';
import type { ContentConfigReport } from './bindings/ContentConfigReport';

const game = {
  id: 'g-a',
  name: 'Game A',
  executable: '/game',
  working_dir: '/game',
  sha256: 'hash',
  version: null,
  branch: null,
  platform: 'macos',
  arch: 'arm64',
  imported_at: '',
};
const instance = {
  id: 'a',
  name: 'A',
  installation_id: game.id,
  installation_sha256: 'hash',
  data_root: '/a',
  binding_status: 'unverified',
  created_at: '',
};
const data = {
  ...emptyDashboard,
  installations: [game, { ...game, id: 'g-b', name: 'Game B' }],
  play_instances: [
    instance,
    {
      ...instance,
      id: 'b',
      name: 'B',
      installation_id: 'g-b',
      data_root: '/b',
    },
  ],
};
function report(id: string): ContentConfigReport {
  return {
    discovery: { scopes: [], packages: [], findings: [], truncated: false },
    instance_id: id,
    data_root: '/' + id,
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
      status: 'missing',
      sha256: null,
      mod_ids: [],
    },
    worlds: [],
    references: [],
    issues: [],
    binding_verified: false,
  };
}
it('limits instances to their installation and discards late reports from a previous target', async () => {
  let resolve!: (value: ContentConfigReport) => void;
  const inspectContentConfig = vi
    .fn()
    .mockImplementationOnce(
      () =>
        new Promise<ContentConfigReport>((r) => {
          resolve = r;
        }),
    )
    .mockResolvedValue(report('b'));
  const user = userEvent.setup();
  render(
    <InstanceContentPanel
      api={{
        ...api,
        desktop: true,
        inspectContentConfig,
        configChanges: vi.fn().mockResolvedValue([]),
      }}
      data={data}
    />,
  );
  expect(
    screen.getByRole('button', { name: '读取并配置此实例' }),
  ).toBeDisabled();
  await user.selectOptions(screen.getByLabelText('目标游戏安装'), 'g-a');
  expect(screen.queryByRole('option', { name: 'B' })).not.toBeInTheDocument();
  await user.selectOptions(screen.getByLabelText('目标游玩实例'), 'a');
  await user.click(screen.getByRole('button', { name: '读取并配置此实例' }));
  await user.selectOptions(screen.getByLabelText('目标游戏安装'), 'g-b');
  expect(screen.getByLabelText('目标游玩实例')).toHaveValue('');
  await act(async () => resolve(report('a')));
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  await user.selectOptions(screen.getByLabelText('目标游玩实例'), 'b');
  await user.click(screen.getByRole('button', { name: '读取并配置此实例' }));
  expect(
    await screen.findByRole('dialog', { name: 'B · 游戏、模组与声音配置' }),
  ).toBeVisible();
  expect(inspectContentConfig).toHaveBeenNthCalledWith(2, 'b');
});
it('rejects a report whose identity does not match the selected instance', async () => {
  const user = userEvent.setup();
  render(
    <InstanceContentPanel
      api={{
        ...api,
        desktop: true,
        inspectContentConfig: vi.fn().mockResolvedValue(report('b')),
      }}
      data={data}
    />,
  );
  await user.selectOptions(screen.getByLabelText('目标游戏安装'), 'g-a');
  await user.selectOptions(screen.getByLabelText('目标游玩实例'), 'a');
  await user.click(screen.getByRole('button', { name: '读取并配置此实例' }));
  expect(await screen.findByRole('alert')).toHaveTextContent(
    '配置报告与目标实例不一致',
  );
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
});
