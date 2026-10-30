import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { expect, it, vi } from 'vitest';
import AutomaticBackupPolicyPanel from './AutomaticBackupPolicyPanel';
import { api } from './api';
import type { AutomaticBackupPolicy } from './bindings/AutomaticBackupPolicy';
const policy: AutomaticBackupPolicy = {
  instance_id: 'a',
  revision: 0,
  settings: {
    enabled: false,
    min_interval_minutes: 60,
    retention: { keep_recent: 7, daily_days: 14, weekly_weeks: 8 },
  },
  updated_at: '',
  last_status: null,
  last_task_id: null,
  last_error_code: null,
};
function client() {
  return {
    ...api,
    desktop: true,
    automaticBackupPolicy: vi.fn().mockResolvedValue(policy),
    saveAutomaticBackupPolicy: vi.fn().mockResolvedValue({
      ...policy,
      revision: 1,
      settings: { ...policy.settings, enabled: true },
    }),
    previewInstanceRetention: vi.fn().mockResolvedValue({
      scope: 'instance_data_root:a',
      items: [],
      protected_count: 0,
      candidate_count: 0,
    }),
  };
}
it('loads disabled policy, requires explicit consent, and saves only the selected instance', async () => {
  const api = client();
  const user = userEvent.setup();
  render(
    <AutomaticBackupPolicyPanel
      api={api}
      instanceId="a"
      busy={false}
      blocked={false}
      run={(f) => f()}
    />,
  );
  await waitFor(() =>
    expect(api.automaticBackupPolicy).toHaveBeenCalledWith('a'),
  );
  await user.click(screen.getByText(/此实例的自动备份与保留策略/));
  const enabled = screen.getByRole('checkbox', {
    name: '正常退出后自动备份此实例',
  });
  await waitFor(() => expect(enabled).toBeEnabled());
  expect(enabled).not.toBeChecked();
  expect(api.saveAutomaticBackupPolicy).not.toHaveBeenCalled();
  await user.click(enabled);
  expect(screen.getByRole('button', { name: '保存此实例策略' })).toBeDisabled();
  await user.click(screen.getByRole('checkbox', { name: /我确认自动备份/ }));
  await user.click(screen.getByRole('button', { name: '保存此实例策略' }));
  expect(api.saveAutomaticBackupPolicy).toHaveBeenCalledWith(
    'a',
    0,
    { ...policy.settings, enabled: true },
    true,
  );
  expect(
    screen.getByRole('checkbox', { name: /我确认自动备份/ }),
  ).not.toBeChecked();
});
it('previews only the chosen instance and rejects a mismatched report', async () => {
  const api = client();
  const user = userEvent.setup();
  render(
    <AutomaticBackupPolicyPanel
      api={api}
      instanceId="a"
      busy={false}
      blocked={false}
      run={(f) => f()}
    />,
  );
  await user.click(screen.getByText(/此实例的自动备份与保留策略/));
  await waitFor(() =>
    expect(
      screen.getByRole('button', { name: '预览此实例保留规则' }),
    ).toBeEnabled(),
  );
  await user.click(screen.getByRole('button', { name: '预览此实例保留规则' }));
  expect(api.previewInstanceRetention).toHaveBeenCalledWith(
    'a',
    policy.settings.retention,
  );
  expect(await screen.findByText(/仅此实例：保留 0 份/)).toBeInTheDocument();
  api.previewInstanceRetention.mockResolvedValueOnce({
    scope: 'workspace',
    items: [],
    protected_count: 99,
    candidate_count: 99,
  });
  await user.click(screen.getByRole('button', { name: '预览此实例保留规则' }));
  expect(await screen.findByRole('alert')).toHaveTextContent(
    '范围与所选实例不符',
  );
  expect(api.saveAutomaticBackupPolicy).not.toHaveBeenCalled();
});
it('permits disabling a policy during resource use but blocks enabling it', async () => {
  const api = client();
  api.automaticBackupPolicy.mockResolvedValue({
    ...policy,
    settings: { ...policy.settings, enabled: true },
  });
  const user = userEvent.setup();
  render(
    <AutomaticBackupPolicyPanel
      api={api}
      instanceId="a"
      busy={false}
      blocked
      run={(f) => f()}
    />,
  );
  await user.click(screen.getByText(/此实例的自动备份与保留策略/));
  const box = screen.getByRole('checkbox', {
    name: '正常退出后自动备份此实例',
  });
  await waitFor(() => expect(box).toBeChecked());
  expect(screen.getByRole('button', { name: '保存此实例策略' })).toBeDisabled();
  await user.click(box);
  expect(screen.getByRole('button', { name: '保存此实例策略' })).toBeEnabled();
  await user.click(screen.getByRole('button', { name: '保存此实例策略' }));
  expect(api.saveAutomaticBackupPolicy).toHaveBeenCalledWith(
    'a',
    0,
    policy.settings,
    true,
  );
});

it('does not label an unreadable saved policy as disabled', async () => {
  const api = client();
  api.automaticBackupPolicy.mockRejectedValue(new Error('无法读取策略'));
  render(
    <AutomaticBackupPolicyPanel
      api={api}
      instanceId="a"
      busy={false}
      blocked={false}
      run={(f) => f()}
    />,
  );
  expect(await screen.findByText(/状态未知（读取失败）/)).toBeInTheDocument();
  await userEvent.setup().click(screen.getByText(/此实例的自动备份与保留策略/));
  expect(screen.getByRole('button', { name: '保存此实例策略' })).toBeDisabled();
  expect(screen.queryByText(/默认关闭/)).not.toBeInTheDocument();
  expect(api.saveAutomaticBackupPolicy).not.toHaveBeenCalled();
});
