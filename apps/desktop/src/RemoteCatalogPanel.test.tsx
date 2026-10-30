import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { expect, it, vi } from 'vitest';
import RemoteCatalogPanel from './RemoteCatalogPanel';
import type { RemoteView, RemoteApi } from './remoteCatalog';
const fixture: RemoteView = {
  sources: [
    { id: 'dda', name: 'DDA', kind: 'game', repo: 'CleverRaven/Cataclysm-DDA' },
    {
      id: 'sound',
      name: '声音',
      kind: 'soundpack',
      repo: 'Fris0uman/CDDA-Soundpacks',
    },
  ],
  catalog: {
    downloads: [],
    listings: {
      dda: {
        attempted: true,
        updated: '2026-10-29T00:00:00Z',
        error: null,
        releases: [
          {
            tag: '0.I',
            prerelease: false,
            published: '2026-10-29',
            assets: [
              {
                id: 7,
                name: 'windows.zip',
                size: 100,
                url: 'https://github.com/CleverRaven/Cataclysm-DDA/releases/download/0.I/windows.zip',
                digest: null,
              },
            ],
          },
        ],
      },
    },
  },
};
function client(): RemoteApi {
  return {
    lookup: vi.fn().mockResolvedValue(fixture),
    openExtraction: vi.fn().mockResolvedValue(undefined),
    pickExtraction: vi.fn().mockResolvedValue(null),
    extract: vi.fn().mockResolvedValue({ path: '/extracted' }),
    progress: vi.fn().mockResolvedValue(null),
    cancel: vi.fn().mockResolvedValue(undefined),
    view: vi.fn().mockResolvedValue(fixture),
    sync: vi.fn().mockResolvedValue(fixture),
    download: vi.fn().mockResolvedValue({ path: '/downloads/windows.zip' }),
    remove: vi.fn().mockResolvedValue(fixture),
  };
}
it('requests first-attempt semantics, separates resource sources, and manually refreshes only on click', async () => {
  const c = client();
  const u = userEvent.setup();
  render(<RemoteCatalogPanel kind="game" client={c} />);
  await waitFor(() => expect(c.sync).toHaveBeenCalledWith('dda', false));
  expect(
    screen.queryByRole('option', { name: '声音' }),
  ).not.toBeInTheDocument();
  await u.click(screen.getByRole('button', { name: '更新版本' }));
  await waitFor(() => expect(c.sync).toHaveBeenLastCalledWith('dda', true));
  expect(c.download).not.toHaveBeenCalled();
});
it('requires an explicit asset confirmation and preserves target ids', async () => {
  const c = client();
  const u = userEvent.setup();
  render(<RemoteCatalogPanel kind="game" client={c} />);
  await screen.findByText('0.I');
  await u.click(screen.getByText('0.I'));
  await u.click(screen.getByRole('button', { name: '下载' }));
  expect(c.download).not.toHaveBeenCalled();
  await u.click(screen.getByRole('button', { name: '确认下载' }));
  await waitFor(() => expect(c.download).toHaveBeenCalledWith('dda', '0.I', 7));
  expect(await screen.findByRole('status')).toHaveTextContent(
    '/downloads/windows.zip',
  );
});
it('keeps cached releases after a manual network failure', async () => {
  const c = client();
  const u = userEvent.setup();
  render(<RemoteCatalogPanel kind="game" client={c} />);
  await screen.findByText('0.I');
  await waitFor(() => {
    expect(c.sync).toHaveBeenCalledWith('dda', false);
    expect(screen.getByRole('button', { name: '更新版本' })).toBeEnabled();
  });
  vi.mocked(c.sync).mockRejectedValueOnce('rate limit');
  await u.click(screen.getByRole('button', { name: '更新版本' }));
  expect(await screen.findByRole('alert')).toHaveTextContent('rate limit');
  expect(screen.getByText('0.I')).toBeInTheDocument();
});
it('does not delete a downloaded file until confirmation', async () => {
  const c = client();
  const v: RemoteView = {
    ...fixture,
    catalog: {
      ...fixture.catalog,
      downloads: [
        {
          id: 'x',
          source: 'dda',
          tag: '0.I',
          name: 'saved.zip',
          path: '/saved.zip',
          size: 100,
          sha256: 'a',
          upstream_verified: false,
        },
      ],
    },
  };
  vi.mocked(c.view).mockResolvedValue(v);
  vi.mocked(c.sync).mockResolvedValue(v);
  const u = userEvent.setup();
  render(<RemoteCatalogPanel kind="game" client={c} />);
  await screen.findByText(/saved.zip ·/);
  await u.click(screen.getByText(/saved.zip ·/));
  await u.click(screen.getByRole('button', { name: '删除下载包' }));
  expect(c.remove).not.toHaveBeenCalled();
  await u.click(screen.getByRole('button', { name: '确认删除下载包' }));
  await waitFor(() => expect(c.remove).toHaveBeenCalledWith('x'));
});
it('offers cancellation while a large download is pending', async () => {
  const c = client();
  let rejectDownload: (e: Error) => void = () => {};
  vi.mocked(c.download).mockImplementation(
    () =>
      new Promise((_resolve, reject) => {
        rejectDownload = reject;
      }),
  );
  vi.mocked(c.cancel).mockImplementation(async () => {
    rejectDownload(new Error('下载已取消'));
  });
  const u = userEvent.setup();
  render(<RemoteCatalogPanel kind="game" client={c} />);
  await screen.findByText('0.I');
  await u.click(screen.getByText('0.I'));
  await u.click(screen.getByRole('button', { name: '下载' }));
  await u.click(screen.getByRole('button', { name: '确认下载' }));
  await u.click(await screen.findByRole('button', { name: '取消下载' }));
  await waitFor(() => expect(c.cancel).toHaveBeenCalledOnce());
  await waitFor(() =>
    expect(screen.getByRole('button', { name: '确认下载' })).toBeEnabled(),
  );
});

it('offers cache verification instead of another download and explicitly confirms extraction', async () => {
  const c = client();
  const v: RemoteView = {
    ...fixture,
    host_os: 'macos',
    host_arch: 'aarch64',
    catalog: {
      ...fixture.catalog,
      downloads: [
        {
          id: 'cached',
          source: 'dda',
          repo: 'CleverRaven/Cataclysm-DDA',
          tag: '0.I',
          asset_id: 7,
          url: fixture.catalog.listings.dda.releases[0].assets[0].url,
          name: 'windows.zip',
          path: '/cached/windows.zip',
          size: 100,
          sha256: 'hash',
          upstream_verified: false,
        },
      ],
    },
  };
  vi.mocked(c.view).mockResolvedValue(v);
  vi.mocked(c.sync).mockResolvedValue(v);
  const u = userEvent.setup();
  render(<RemoteCatalogPanel kind="game" client={c} />);
  await screen.findByText('0.I');
  await u.click(screen.getByText('0.I'));
  expect(screen.getByRole('button', { name: '校验并复用缓存' })).toBeEnabled();
  expect(screen.getByText(/文件名提示：其他系统或架构/)).toBeInTheDocument();
  expect(c.download).not.toHaveBeenCalled();
  await u.click(screen.getByText(/windows.zip ·/));
  await u.click(screen.getByRole('button', { name: '解压到独立目录' }));
  expect(c.extract).not.toHaveBeenCalled();
  await u.click(screen.getByRole('button', { name: '确认解压' }));
  await waitFor(() => expect(c.extract).toHaveBeenCalledWith('cached'));
  expect(await screen.findByRole('status')).toHaveTextContent('/extracted');
});

it('recovers an existing transfer after remount and retains cancellation', async () => {
  const c = client();
  vi.mocked(c.progress).mockResolvedValue({
    name: 'active.zip',
    received: 12,
    total: 100,
  });
  const first = render(<RemoteCatalogPanel kind="game" client={c} />);
  await screen.findByText(/正在下载：active.zip/);
  first.unmount();
  render(<RemoteCatalogPanel kind="game" client={c} />);
  await screen.findByText(/正在下载：active.zip/);
  expect(c.download).not.toHaveBeenCalled();
  await userEvent
    .setup()
    .click(screen.getByRole('button', { name: '取消下载' }));
  expect(c.cancel).toHaveBeenCalledOnce();
});

it('only queries a historical tag on explicit action and never starts a download', async () => {
  const c = client();
  const u = userEvent.setup();
  render(<RemoteCatalogPanel kind="game" client={c} />);
  await waitFor(() =>
    expect(screen.getByRole('button', { name: '更新版本' })).toBeEnabled(),
  );
  await u.type(screen.getByLabelText('查找历史发布标签'), '0.H');
  expect(c.lookup).not.toHaveBeenCalled();
  await u.click(screen.getByRole('button', { name: '查找历史版本' }));
  await waitFor(() => expect(c.lookup).toHaveBeenCalledWith('dda', '0.H'));
  expect(c.download).not.toHaveBeenCalled();
});

it('opens extraction directories and requires native selection before import', async () => {
  const c = client();
  const u = userEvent.setup();
  const onImport = vi.fn();
  const v = {
    ...fixture,
    catalog: {
      ...fixture.catalog,
      extractions: [
        {
          id: 'e',
          source: 'dda',
          name: 'game.zip',
          tag: '0.H',
          download_id: 'd',
          archive_sha256: 'hash',
          path: '/cache/extracted',
          entries: 1,
          bytes: 10,
          extracted_at: 'now',
        },
      ],
    },
  };
  vi.mocked(c.view).mockResolvedValue(v);
  vi.mocked(c.sync).mockResolvedValue(v);
  render(<RemoteCatalogPanel kind="game" client={c} onImport={onImport} />);
  await screen.findByText('/cache/extracted');
  await u.click(screen.getByRole('button', { name: '打开解压目录' }));
  expect(c.openExtraction).toHaveBeenCalledWith('e');
  await u.click(screen.getByRole('button', { name: '选择并导入游戏' }));
  expect(c.pickExtraction).toHaveBeenCalledWith('e', 'executable');
  expect(onImport).not.toHaveBeenCalled();
  vi.mocked(c.pickExtraction).mockResolvedValue('/cache/extracted/game.app');
  await u.click(screen.getByRole('button', { name: '选择并导入游戏' }));
  await waitFor(() =>
    expect(onImport).toHaveBeenCalledWith('/cache/extracted/game.app', 'mod'),
  );
});
