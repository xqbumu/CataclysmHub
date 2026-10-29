import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { expect, it, vi } from 'vitest';
import HubSettings from './HubSettings';
import { api } from './api';

it('requires confirmation and reports the pre-reset backup only after a successful reset', async () => {
  const user = userEvent.setup();
  const onReset = vi.fn();
  const resetPreferences = vi.fn().mockResolvedValue({
    backup_path: '/metadata/config-backups/old.json',
    preferences: { revision: 8, instance_profiles: {} },
  });
  render(
    <HubSettings
      api={{
        ...api,
        desktop: true,
        preferences: vi.fn().mockResolvedValue({
          revision: 7,
          instance_profiles: { a: 'profile' },
        }),
        resetPreferences,
      }}
      onReset={onReset}
    />,
  );
  await user.click(screen.getByRole('button', { name: '预览配置重置' }));
  expect(
    await screen.findByRole('button', { name: '备份并重置' }),
  ).toBeDisabled();
  expect(resetPreferences).not.toHaveBeenCalled();
  await user.click(screen.getByRole('checkbox'));
  await user.click(screen.getByRole('button', { name: '备份并重置' }));
  await waitFor(() => expect(onReset).toHaveBeenCalledOnce());
  expect(resetPreferences).toHaveBeenCalledWith(7, true);
  expect(screen.getByRole('status')).toHaveTextContent(
    '/metadata/config-backups/old.json',
  );
});
it('keeps the confirmation and current UI state when backup/reset fails', async () => {
  const user = userEvent.setup();
  const onReset = vi.fn();
  render(
    <HubSettings
      api={{
        ...api,
        desktop: true,
        preferences: vi
          .fn()
          .mockResolvedValue({ revision: 1, instance_profiles: {} }),
        resetPreferences: vi.fn().mockRejectedValue(new Error('配置备份失败')),
      }}
      onReset={onReset}
    />,
  );
  await user.click(screen.getByRole('button', { name: '预览配置重置' }));
  await user.click(await screen.findByRole('checkbox'));
  await user.click(screen.getByRole('button', { name: '备份并重置' }));
  expect(await screen.findByRole('alert')).toHaveTextContent('配置备份失败');
  expect(onReset).not.toHaveBeenCalled();
  expect(screen.getByRole('dialog')).toBeVisible();
});
