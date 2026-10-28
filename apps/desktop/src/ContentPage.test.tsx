import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { it, expect, vi } from 'vitest';
import ContentPage from './ContentPage';
import { api as native, type HubApi } from './api';
import type { ContentEntry } from './bindings/ContentEntry';
const entry: ContentEntry = {
  id: 'one',
  kind: 'mod',
  root: '/mods/example',
  definitions: [
    {
      content_id: 'example',
      name: 'Example',
      dependencies: ['dda'],
      version: null,
    },
  ],
  metadata_sha256: 'a'.repeat(64),
  checked_at: '2026-10-28',
  last_error: null,
};
function client(entries: ContentEntry[] = []): HubApi {
  return {
    ...native,
    desktop: true,
    contentEntries: vi.fn().mockResolvedValue(entries),
    pickPath: vi.fn().mockResolvedValue('/mods/example'),
    registerContent: vi.fn().mockResolvedValue(entry),
    refreshContent: vi.fn().mockResolvedValue(entry),
    forgetContent: vi.fn().mockResolvedValue(undefined),
  };
}
it('registers a chosen package type without installing or enabling it', async () => {
  const api = client();
  const user = userEvent.setup();
  render(<ContentPage api={api} />);
  await waitFor(() => expect(api.contentEntries).toHaveBeenCalledOnce());
  await user.selectOptions(screen.getByLabelText('资源类型'), 'soundpack');
  await user.click(screen.getByRole('button', { name: '登记本地包目录' }));
  expect(api.registerContent).toHaveBeenCalledWith(
    '/mods/example',
    'soundpack',
  );
  expect(screen.getByText(/不复制、安装或启用资源/)).toBeInTheDocument();
});
it('shows duplicate IDs and unresolved catalog dependencies and requires confirmation to forget', async () => {
  const api = client([entry, { ...entry, id: 'two', root: '/mods/other' }]);
  const user = userEvent.setup();
  render(<ContentPage api={api} />);
  expect(await screen.findAllByText(/重复 ID/)).toHaveLength(2);
  expect(screen.getAllByText(/未在资源库找到：dda/)).toHaveLength(2);
  await user.click(screen.getAllByRole('button', { name: '移除登记' })[0]);
  expect(api.forgetContent).not.toHaveBeenCalled();
  expect(
    screen.getByText(/不删除文件，也不修改世界或声音配置/),
  ).toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: '取消' }));
  expect(api.forgetContent).not.toHaveBeenCalled();
  await user.click(screen.getAllByRole('button', { name: '移除登记' })[0]);
  await user.click(screen.getByRole('button', { name: '确认移除登记' }));
  expect(api.forgetContent).toHaveBeenCalledWith('one');
});
it('labels failed refresh data as historical and supports picker cancellation', async () => {
  const api = client([{ ...entry, last_error: 'PATH_INVALID' }]);
  vi.mocked(api.pickPath).mockResolvedValue(null);
  render(<ContentPage api={api} />);
  const user = userEvent.setup();
  expect(await screen.findByRole('alert')).toHaveTextContent('历史元数据');
  await user.click(screen.getByRole('button', { name: '登记本地包目录' }));
  expect(api.registerContent).not.toHaveBeenCalled();
  await user.click(screen.getByRole('button', { name: '刷新元数据' }));
  expect(api.refreshContent).toHaveBeenCalledWith('one');
});
