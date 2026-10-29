import { act, render, screen, within, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { expect, it, vi } from 'vitest';
import TaskCenter from './TaskCenter';
import { api, emptyDashboard, type Dashboard, type HubApi } from './api';
const base = {
  id: 'same-id',
  request_key: 'key',
  workspace_id: 'workspace',
  state: 'running',
  phase: 'preflight',
  revision: 0,
  error_code: null,
  created_at: '2026-10-29T00:00:00Z',
  finished_at: null,
};
function client(): HubApi {
  return {
    ...api,
    desktop: true,
    contentInstallTasks: vi.fn().mockResolvedValue([]),
  };
}
const data: Dashboard = {
  ...emptyDashboard,
  scan_tasks: [{ ...base, operation: 'world.scan', processed_entries: 0 }],
  backup_tasks: [
    {
      ...base,
      state: 'cancelling',
      operation: 'backup.verify',
      backup_id: 'archive',
      processed_entries: 0,
    },
  ],
  restore_tasks: [
    {
      ...base,
      state: 'recovery_required',
      backup_id: 'archive',
      destination: '/restore',
      error_code: 'E_INTERRUPTED',
    },
  ],
};
it('prioritizes interrupted tasks, keeps cancelling active, and routes without executing actions', async () => {
  const onOpen = vi.fn();
  const user = userEvent.setup();
  render(<TaskCenter data={data} api={client()} onOpen={onOpen} />);
  await screen.findByText('需要处理 1 · 执行中 2 · 已结束 0');
  const articles = screen.getAllByRole('article');
  expect(within(articles[0]).getByRole('heading')).toHaveTextContent(
    '恢复 · 中断，需要检查',
  );
  await user.selectOptions(screen.getByLabelText('任务状态'), 'active');
  expect(screen.getAllByRole('article')).toHaveLength(2);
  await user.click(screen.getByRole('button', { name: '打开归档校验管理' }));
  expect(onOpen).toHaveBeenCalledWith('archives');
  await user.click(screen.getByRole('button', { name: '打开世界扫描管理' }));
  expect(onOpen).toHaveBeenLastCalledWith('worlds');
});
it('searches errors and treats unknown states as needing attention', async () => {
  const user = userEvent.setup();
  render(
    <TaskCenter
      data={{
        ...data,
        scan_tasks: [{ ...data.scan_tasks[0], state: 'new_backend_state' }],
      }}
      api={client()}
      onOpen={vi.fn()}
    />,
  );
  await user.selectOptions(screen.getByLabelText('任务状态'), 'attention');
  expect(screen.getByRole('heading', { name: /未知状态/ })).toBeVisible();
  await user.type(screen.getByLabelText('搜索任务'), 'E_INTERRUPTED');
  expect(screen.getAllByRole('article')).toHaveLength(1);
  expect(screen.getByText('错误码：E_INTERRUPTED')).toBeVisible();
});
it('reports partial read failures and retries without hiding dashboard tasks', async () => {
  const service = client();
  const user = userEvent.setup();
  vi.mocked(service.contentInstallTasks)
    .mockRejectedValueOnce(new Error('offline'))
    .mockResolvedValue([]);
  render(<TaskCenter data={data} api={service} onOpen={vi.fn()} />);
  expect(await screen.findByRole('alert')).toHaveTextContent('列表可能不完整');
  expect(screen.getAllByRole('article')).toHaveLength(3);
  await user.click(screen.getByRole('button', { name: '重试资源任务读取' }));
  await screen.findByText('需要处理 1 · 执行中 2 · 已结束 0');
  await waitFor(() =>
    expect(screen.queryByRole('alert')).not.toBeInTheDocument(),
  );
});

it('includes resource installations and routes completed tasks to content', async () => {
  const service = client();
  const onOpen = vi.fn();
  const user = userEvent.setup();
  vi.mocked(service.contentInstallTasks).mockResolvedValue([
    {
      ...base,
      id: 'resource-task',
      entry_id: 'mod',
      source_root: '/mods',
      destination: '/library/package',
      payload_root: '/library/package/payload',
      manifest_sha256: 'a',
      state: 'succeeded',
    },
  ]);
  render(<TaskCenter data={emptyDashboard} api={service} onOpen={onOpen} />);
  await user.selectOptions(screen.getByLabelText('任务状态'), 'finished');
  expect(
    await screen.findByRole('heading', { name: '资源安装 · 任务完成' }),
  ).toBeVisible();
  await user.type(screen.getByLabelText('搜索任务'), '/library/package');
  await user.click(screen.getByRole('button', { name: '打开资源安装管理' }));
  expect(onOpen).toHaveBeenCalledWith('content');
});

it('retains the last resource task when a polling request fails', async () => {
  vi.useFakeTimers();
  try {
    const service = client();
    vi.mocked(service.contentInstallTasks)
      .mockResolvedValueOnce([
        {
          ...base,
          id: 'retained-task',
          entry_id: 'mod',
          source_root: '/mods',
          destination: '/library/package',
          payload_root: '/library/package/payload',
          manifest_sha256: 'a',
        },
      ])
      .mockRejectedValue(new Error('temporary read failure'));
    const view = render(
      <TaskCenter data={emptyDashboard} api={service} onOpen={vi.fn()} />,
    );
    await act(async () => {
      await Promise.resolve();
    });
    expect(
      screen.getByRole('heading', { name: '资源安装 · 执行中' }),
    ).toBeVisible();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2000);
    });
    expect(screen.getByRole('alert')).toHaveTextContent('上次读取结果');
    expect(
      screen.getByRole('heading', { name: '资源安装 · 执行中' }),
    ).toBeVisible();
    view.unmount();
    await vi.advanceTimersByTimeAsync(4000);
    expect(service.contentInstallTasks).toHaveBeenCalledTimes(2);
  } finally {
    vi.useRealTimers();
  }
});
