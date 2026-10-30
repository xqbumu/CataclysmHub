import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { expect, it, vi } from 'vitest';
import { api } from './api';
import InstallationQuarantineAction from './InstallationQuarantineAction';
const plan = {
  id: 'plan',
  installation_id: 'game',
  scope: '/QA/Game.app',
  executable_sha256: 'hash',
  quarantine_entries: 12,
  backup_directory: '/metadata/installation-security/plan',
  expires_at: '2030-01-01T00:00:00Z',
};
function show(disabled = false, failure = false) {
  const prepareInstallationQuarantine = vi.fn().mockResolvedValue(plan);
  const applyInstallationQuarantine = vi.fn();
  if (failure)
    applyInstallationQuarantine.mockRejectedValue(
      new Error('QUARANTINE_PLAN_STALE'),
    );
  else
    applyInstallationQuarantine.mockResolvedValue({
      backup_directory: plan.backup_directory,
      changed_entries: 12,
    });
  const onRefresh = vi.fn().mockResolvedValue(undefined);
  const launch = vi.fn();
  render(
    <InstallationQuarantineAction
      api={{
        ...api,
        desktop: true,
        launch,
        prepareInstallationQuarantine,
        applyInstallationQuarantine,
      }}
      installationId="game"
      disabled={disabled}
      onRefresh={onRefresh}
    />,
  );
  return {
    prepareInstallationQuarantine,
    applyInstallationQuarantine,
    onRefresh,
    launch,
  };
}
it('requires separate consent, then refreshes without launching', async () => {
  const calls = show();
  const u = userEvent.setup();
  await u.click(screen.getByRole('button', { name: '检查此安装的隔离属性' }));
  const d = await screen.findByRole('dialog');
  expect(d).toHaveTextContent(plan.scope);
  expect(d).toHaveTextContent('不属于正常 Gatekeeper 授权');
  const confirm = within(d).getByRole('button', {
    name: '确认仅移除此安装的隔离属性',
  });
  expect(confirm).toBeDisabled();
  expect(calls.applyInstallationQuarantine).not.toHaveBeenCalled();
  await u.click(within(d).getByRole('checkbox'));
  await u.click(confirm);
  expect(await screen.findByRole('status')).toHaveTextContent('已处理 12 项');
  expect(calls.applyInstallationQuarantine).toHaveBeenCalledExactlyOnceWith(
    'plan',
  );
  expect(calls.onRefresh).toHaveBeenCalledOnce();
  expect(calls.launch).not.toHaveBeenCalled();
});
it('cancel is read-only and requires new consent when reopening', async () => {
  const calls = show();
  const u = userEvent.setup();
  await u.click(screen.getByRole('button', { name: '检查此安装的隔离属性' }));
  let d = await screen.findByRole('dialog');
  await u.click(within(d).getByRole('checkbox'));
  await u.click(within(d).getByRole('button', { name: '取消' }));
  expect(calls.applyInstallationQuarantine).not.toHaveBeenCalled();
  await u.click(screen.getByRole('button', { name: '检查此安装的隔离属性' }));
  d = await screen.findByRole('dialog');
  expect(within(d).getByRole('checkbox')).not.toBeChecked();
});
it('blocked resources cannot start a preview', () => {
  const calls = show(true);
  expect(screen.getByRole('button')).toBeDisabled();
  expect(calls.prepareInstallationQuarantine).not.toHaveBeenCalled();
});
it('does not retry a stale confirmation or claim success', async () => {
  const calls = show(false, true);
  const u = userEvent.setup();
  await u.click(screen.getByRole('button'));
  const d = await screen.findByRole('dialog');
  await u.click(within(d).getByRole('checkbox'));
  await u.click(
    within(d).getByRole('button', { name: '确认仅移除此安装的隔离属性' }),
  );
  expect(await screen.findByRole('alert')).toHaveTextContent(
    'QUARANTINE_PLAN_STALE',
  );
  expect(calls.applyInstallationQuarantine).toHaveBeenCalledOnce();
  expect(
    within(d).getByRole('button', { name: '确认仅移除此安装的隔离属性' }),
  ).toBeDisabled();
  expect(calls.onRefresh).not.toHaveBeenCalled();
  expect(screen.queryByRole('status')).not.toBeInTheDocument();
});
