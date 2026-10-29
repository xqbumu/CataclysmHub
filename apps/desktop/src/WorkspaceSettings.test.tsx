import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { it, expect, vi } from 'vitest';
import WorkspaceSettings from './WorkspaceSettings';
import { api } from './api';
const workspace = {
  id: 'w',
  root_dir: '/old',
  save_dir: '/old/saves',
  backup_dir: '/old/backups',
  content_dir: '/old/content',
  created_at: 'date',
};
it('configures all default directories from one native root selection', async () => {
  const user = userEvent.setup();
  const configureWorkspace = vi.fn().mockResolvedValue(workspace);
  const pickPath = vi.fn().mockResolvedValue('/new');
  const onSaved = vi.fn().mockResolvedValue(undefined);
  render(
    <WorkspaceSettings
      api={{ ...api, desktop: true, pickPath, configureWorkspace }}
      workspace={null}
      defaults={null}
      onSaved={onSaved}
    />,
  );
  await user.click(screen.getByRole('button', { name: '选择根目录' }));
  await user.click(screen.getByRole('button', { name: '保存工作区' }));
  await waitFor(() =>
    expect(configureWorkspace).toHaveBeenCalledWith(
      '/new',
      {
        save_dir: '/new/saves',
        backup_dir: '/new/backups',
        content_dir: '/new/content',
      },
      null,
    ),
  );
  expect(pickPath).toHaveBeenCalledOnce();
  expect(onSaved).toHaveBeenCalledOnce();
});
it('edits a configured workspace with explicit acknowledgement and reports backend failure', async () => {
  const user = userEvent.setup();
  const configureWorkspace = vi
    .fn()
    .mockRejectedValue(new Error('已有备份，保留原目录'));
  const onSaved = vi.fn();
  render(
    <WorkspaceSettings
      api={{ ...api, desktop: true, configureWorkspace }}
      workspace={workspace}
      defaults={null}
      onSaved={onSaved}
    />,
  );
  expect(screen.getByRole('button', { name: '选择根目录' })).toBeEnabled();
  await user.click(screen.getByRole('checkbox'));
  await user.click(screen.getByRole('button', { name: '保存目录设置' }));
  expect(await screen.findByRole('alert')).toHaveTextContent('已有备份');
  expect(onSaved).not.toHaveBeenCalled();
  expect(configureWorkspace).toHaveBeenCalledWith(
    '/old',
    {
      save_dir: '/old/saves',
      backup_dir: '/old/backups',
      content_dir: '/old/content',
    },
    {
      save_dir: '/old/saves',
      backup_dir: '/old/backups',
      content_dir: '/old/content',
    },
  );
});

it('preserves a root draft when dashboard polling returns identical saved values', async () => {
  const user = userEvent.setup();
  const service = {
    ...api,
    desktop: true,
    pickPath: vi.fn().mockResolvedValue('/draft'),
  };
  const onSaved = vi.fn();
  const { rerender } = render(
    <WorkspaceSettings
      api={service}
      workspace={workspace}
      defaults={null}
      onSaved={onSaved}
    />,
  );
  await user.click(screen.getByRole('button', { name: '选择根目录' }));
  expect(screen.getByLabelText('数据根目录')).toHaveValue('/draft');
  rerender(
    <WorkspaceSettings
      api={service}
      workspace={{ ...workspace }}
      defaults={null}
      onSaved={onSaved}
    />,
  );
  expect(screen.getByLabelText('数据根目录')).toHaveValue('/draft');
});

it('does not replace a first-run draft when platform defaults arrive late', async () => {
  const user = userEvent.setup();
  const service = {
    ...api,
    desktop: true,
    pickPath: vi.fn().mockResolvedValue('/chosen'),
  };
  const onSaved = vi.fn();
  const { rerender } = render(
    <WorkspaceSettings
      api={service}
      workspace={null}
      defaults={null}
      onSaved={onSaved}
    />,
  );
  await user.click(screen.getByRole('button', { name: '选择根目录' }));
  rerender(
    <WorkspaceSettings
      api={service}
      workspace={null}
      defaults={{
        save_dir: '/system/saves',
        backup_dir: '/system/backups',
        content_dir: '/system/content',
      }}
      onSaved={onSaved}
    />,
  );
  expect(screen.getByLabelText('数据根目录')).toHaveValue('/chosen');
  expect(screen.getByLabelText('存档目录')).toHaveValue('/chosen/saves');
});

it('keeps the registered backup directory when selecting a new root until explicitly changed', async () => {
  const user = userEvent.setup();
  const configureWorkspace = vi.fn().mockResolvedValue(workspace);
  const service = {
    ...api,
    desktop: true,
    pickPath: vi.fn().mockResolvedValue('/new'),
    configureWorkspace,
  };
  render(
    <WorkspaceSettings
      api={service}
      workspace={workspace}
      defaults={null}
      onSaved={vi.fn()}
    />,
  );
  await user.click(screen.getByRole('button', { name: '选择根目录' }));
  expect(screen.getByLabelText('独立备份目录')).toHaveValue('/old/backups');
  await user.click(screen.getByRole('checkbox'));
  await user.click(screen.getByRole('button', { name: '保存目录设置' }));
  await waitFor(() =>
    expect(configureWorkspace).toHaveBeenCalledWith(
      '/new',
      {
        save_dir: '/new/saves',
        backup_dir: '/old/backups',
        content_dir: '/new/content',
      },
      {
        save_dir: '/old/saves',
        backup_dir: '/old/backups',
        content_dir: '/old/content',
      },
    ),
  );
});
