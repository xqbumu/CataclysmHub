import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { it, expect, vi } from 'vitest';
import ConfigChangePanel, { ConfigChangeHistory } from './ConfigChangePanel';
import { emptyDashboard } from './api';
import type { ConfigChangePlan } from './bindings/ConfigChangePlan';
import type { ConfigChange } from './bindings/ConfigChange';
const plan: ConfigChangePlan = {
  id: 'plan',
  plan_digest: 'digest',
  instance_id: 'instance',
  backup_id: 'backup',
  data_root: '/instance',
  path: 'config/options.json',
  before_sha256: 'before',
  after_sha256: 'after',
  proposed_json: '[{"name":"SOUND_ENABLED","value":"true"}]',
  expires_at: 'later',
};
const record: ConfigChange = {
  id: 'plan',
  instance_id: 'instance',
  backup_id: 'backup',
  data_root: '/instance',
  path: plan.path,
  before_sha256: 'before',
  after_sha256: 'after',
  state: 'recovery_required',
  error_code: 'CONFIG_CHANGE_INTERRUPTED',
  created_at: 'now',
  finished_at: null,
};
it('requires a selected backup, refreshed plan and explicit file application confirmation', async () => {
  const user = userEvent.setup();
  const prepareConfigChange = vi.fn().mockResolvedValue(plan);
  const applyConfigChange = vi
    .fn()
    .mockResolvedValue({ ...record, state: 'applying', error_code: null });
  const dashboard = vi.fn().mockResolvedValue({
    ...emptyDashboard,
    backups: [
      {
        id: 'backup',
        play_instance_id: 'instance',
        display_name: 'Protection',
        created_at: 'now',
      },
    ],
  });
  render(
    <ConfigChangePanel
      api={{
        dashboard,
        prepareConfigChange,
        prepareConfigRollback: vi.fn(),
        applyConfigChange,
        configChanges: vi.fn(),
        inspectConfigChange: vi.fn(),
        resolveConfigChange: vi.fn(),
      }}
      instanceId="instance"
      input={{ default_mods: null, soundpack: null, sound_enabled: true }}
    />,
  );
  expect(dashboard).not.toHaveBeenCalled();
  expect(prepareConfigChange).not.toHaveBeenCalled();
  await user.click(
    screen.getByRole('button', { name: '读取该实例的保护备份' }),
  );
  await user.selectOptions(
    screen.getByLabelText('用于恢复原配置的备份'),
    'backup',
  );
  await user.click(
    screen.getByRole('button', { name: '校验备份并准备单文件应用' }),
  );
  expect(prepareConfigChange).toHaveBeenCalledWith(
    'instance',
    { default_mods: null, soundpack: null, sound_enabled: true },
    'backup',
  );
  expect(applyConfigChange).not.toHaveBeenCalled();
  expect(
    screen.getByRole('button', { name: '确认应用单文件配置' }),
  ).toBeDisabled();
  expect(screen.getByLabelText('单文件应用确认')).toHaveTextContent(
    '/instance/config/options.json',
  );
  await user.click(
    screen.getByLabelText('我已核对目标与备份，确认应用这个配置文件'),
  );
  await user.click(screen.getByRole('button', { name: '确认应用单文件配置' }));
  expect(applyConfigChange).toHaveBeenCalledExactlyOnceWith(
    'plan',
    'digest',
    expect.any(String),
  );
  expect(screen.getByText(/此响应不代表游戏加载成功/)).toBeInTheDocument();
});
it('requires read-only inspection before explicitly resolving an interrupted publication', async () => {
  const user = userEvent.setup();
  const configChanges = vi.fn().mockResolvedValue([record]);
  const inspectConfigChange = vi.fn().mockResolvedValue({
    change: record,
    current_sha256: 'after',
    matches: 'proposed',
  });
  const resolveConfigChange = vi.fn().mockResolvedValue({
    change: { ...record, state: 'resolved_proposed' },
    current_sha256: 'after',
    matches: 'proposed',
  });
  render(
    <ConfigChangeHistory
      api={{
        configChanges,
        inspectConfigChange,
        resolveConfigChange,
        prepareConfigRollback: vi.fn(),
        applyConfigChange: vi.fn(),
      }}
    />,
  );
  expect(configChanges).not.toHaveBeenCalled();
  await user.click(screen.getByRole('button', { name: '读取配置应用记录' }));
  expect(
    screen.queryByRole('button', { name: '确认保留当前配置并解除中断保护' }),
  ).not.toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: '核对当前配置 plan' }));
  expect(screen.getByLabelText('配置中断核对')).toHaveTextContent(
    '与拟应用配置一致',
  );
  expect(resolveConfigChange).not.toHaveBeenCalled();
  await user.click(
    screen.getByRole('button', { name: '确认保留当前配置并解除中断保护' }),
  );
  expect(resolveConfigChange).toHaveBeenCalledExactlyOnceWith('plan', 'after');
  expect(
    screen.queryByRole('button', { name: '确认保留当前配置并解除中断保护' }),
  ).not.toBeInTheDocument();
});
it('keeps protection when the current file matches neither expected version', async () => {
  const user = userEvent.setup();
  const resolveConfigChange = vi.fn();
  render(
    <ConfigChangeHistory
      api={{
        prepareConfigRollback: vi.fn(),
        applyConfigChange: vi.fn(),
        configChanges: vi.fn().mockResolvedValue([record]),
        inspectConfigChange: vi.fn().mockResolvedValue({
          change: record,
          current_sha256: 'third',
          matches: 'other',
        }),
        resolveConfigChange,
      }}
    />,
  );
  await user.click(screen.getByRole('button', { name: '读取配置应用记录' }));
  await user.click(screen.getByRole('button', { name: '核对当前配置 plan' }));
  expect(screen.getByText(/均不同，不能解除保护/)).toBeInTheDocument();
  expect(
    screen.queryByRole('button', { name: '确认保留当前配置并解除中断保护' }),
  ).not.toBeInTheDocument();
  expect(resolveConfigChange).not.toHaveBeenCalled();
});

it('previews rollback without writing and requires a separate original-byte confirmation', async () => {
  const user = userEvent.setup();
  const original = { ...record, state: 'applied' };
  const rollback = {
    ...plan,
    id: 'rollback',
    rollback_of: original.id,
    before_sha256: 'after',
    after_sha256: 'before',
    proposed_json: '{ "original": true }',
  };
  const configChanges = vi.fn().mockResolvedValue([original]);
  const prepareConfigRollback = vi.fn().mockResolvedValue(rollback);
  const applyConfigChange = vi.fn().mockResolvedValue({
    ...record,
    id: 'rollback',
    rollback_of: original.id,
    state: 'applying',
  });
  render(
    <ConfigChangeHistory
      api={{
        configChanges,
        prepareConfigRollback,
        applyConfigChange,
        inspectConfigChange: vi.fn(),
        resolveConfigChange: vi.fn(),
      }}
    />,
  );
  await user.click(screen.getByRole('button', { name: '读取配置应用记录' }));
  await user.click(screen.getByRole('button', { name: '预览原字节回滚 plan' }));
  expect(prepareConfigRollback).toHaveBeenCalledExactlyOnceWith('plan');
  expect(applyConfigChange).not.toHaveBeenCalled();
  expect(screen.getByLabelText('原字节回滚确认')).toHaveTextContent(
    '恢复后摘要：before',
  );
  expect(
    screen.getByRole('button', { name: '确认原地回滚单文件' }),
  ).toBeDisabled();
  await user.click(screen.getByLabelText('我确认恢复上述原配置字节'));
  await user.click(screen.getByRole('button', { name: '确认原地回滚单文件' }));
  expect(applyConfigChange).toHaveBeenCalledExactlyOnceWith(
    'rollback',
    'digest',
    expect.any(String),
  );
  expect(screen.queryByLabelText('原字节回滚确认')).not.toBeInTheDocument();
});

it('does not offer rollback of unresolved records or another rollback', async () => {
  const user = userEvent.setup();
  const prepareConfigRollback = vi.fn();
  render(
    <ConfigChangeHistory
      api={{
        configChanges: vi.fn().mockResolvedValue([
          record,
          {
            ...record,
            id: 'rollback',
            state: 'applied',
            rollback_of: record.id,
          },
        ]),
        prepareConfigRollback,
        applyConfigChange: vi.fn(),
        inspectConfigChange: vi.fn(),
        resolveConfigChange: vi.fn(),
      }}
    />,
  );
  await user.click(screen.getByRole('button', { name: '读取配置应用记录' }));
  expect(
    screen.queryByRole('button', { name: /预览原字节回滚/ }),
  ).not.toBeInTheDocument();
  expect(screen.getByText('回滚来源操作：plan')).toBeInTheDocument();
  expect(prepareConfigRollback).not.toHaveBeenCalled();
});

it('labels interrupted rollback observations relative to the rollback direction', async () => {
  const user = userEvent.setup();
  const rollbackRecord = { ...record, id: 'rollback', rollback_of: 'plan' };
  render(
    <ConfigChangeHistory
      api={{
        configChanges: vi.fn().mockResolvedValue([rollbackRecord]),
        prepareConfigRollback: vi.fn(),
        applyConfigChange: vi.fn(),
        inspectConfigChange: vi.fn().mockResolvedValue({
          change: rollbackRecord,
          current_sha256: 'after',
          matches: 'original',
        }),
        resolveConfigChange: vi.fn(),
      }}
    />,
  );
  await user.click(screen.getByRole('button', { name: '读取配置应用记录' }));
  await user.click(
    screen.getByRole('button', { name: '核对当前配置 rollback' }),
  );
  expect(screen.getByLabelText('配置中断核对')).toHaveTextContent(
    '与本次回滚前的配置一致',
  );
});
