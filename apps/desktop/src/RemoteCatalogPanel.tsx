import { useEffect, useState } from 'react';
import {
  remoteApi,
  type RemoteApi,
  type RemoteView,
  type RemoteAsset,
  type RemoteRelease,
  type RemoteTransfer,
} from './remoteCatalog';
import Modal from './Modal';
const message = (e: unknown) =>
  e instanceof Error
    ? e.message
    : typeof e === 'string'
      ? e
      : '远端操作失败，请手动重试。';
const size = (n: number) =>
  n ? `${(n / 1048576).toFixed(1)} MiB` : '大小未知';
export default function RemoteCatalogPanel({
  kind,
  client = remoteApi,
}: {
  kind: 'game' | 'resource';
  client?: RemoteApi;
}) {
  const [view, setView] = useState<RemoteView | null>(null);
  const [selected, setSelected] = useState('');
  const [query, setQuery] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [pending, setPending] = useState<{
    source: string;
    release: RemoteRelease;
    asset: RemoteAsset;
  } | null>(null);
  const [notice, setNotice] = useState('');
  const [transfer, setTransfer] = useState<RemoteTransfer | null>(null);
  const [downloading, setDownloading] = useState(false);
  useEffect(() => {
    if (!downloading) return;
    let alive = true;
    const timer = setInterval(() => {
      void client
        .progress()
        .then((p) => {
          if (alive) setTransfer(p);
        })
        .catch(() => {});
    }, 500);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, [client, downloading]);
  const [removing, setRemoving] = useState<string | null>(null);
  useEffect(() => {
    let alive = true;
    void client
      .view()
      .then((v) => {
        if (alive) {
          setView(v);
          setSelected(
            v.sources.find((s) =>
              kind === 'game' ? s.kind === 'game' : s.kind !== 'game',
            )?.id ?? '',
          );
        }
      })
      .catch((e) => {
        if (alive) setError(message(e));
      });
    return () => {
      alive = false;
    };
  }, [client, kind]);
  useEffect(() => {
    if (!selected) return;
    let alive = true;
    setBusy(true);
    void client
      .sync(selected, false)
      .then((v) => {
        if (alive) setView(v);
      })
      .catch((e) => {
        if (alive) setError(message(e));
      })
      .finally(() => {
        if (alive) setBusy(false);
      });
    return () => {
      alive = false;
    };
  }, [client, selected]);
  const sources =
    view?.sources.filter((s) =>
      kind === 'game' ? s.kind === 'game' : s.kind !== 'game',
    ) ?? [];
  const source = sources.find((s) => s.id === selected);
  const listing = view?.catalog.listings[selected];
  async function update() {
    setBusy(true);
    setError('');
    try {
      setView(await client.sync(selected, true));
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
    }
  }
  async function download() {
    if (!pending) return;
    setDownloading(true);
    setTransfer(null);
    setBusy(true);
    setError('');
    try {
      const saved = await client.download(
        pending.source,
        pending.release.tag,
        pending.asset.id,
      );
      setNotice(`已下载：${saved.path}`);
      setPending(null);
      setView(await client.view());
    } catch (e) {
      setError(message(e));
    } finally {
      setBusy(false);
      setDownloading(false);
      setTransfer(null);
    }
  }
  const releases =
    listing?.releases
      .map((r) => ({
        ...r,
        assets: r.assets.filter((a) =>
          `${r.tag} ${a.name}`.toLowerCase().includes(query.toLowerCase()),
        ),
      }))
      .filter((r) => r.assets.length || (!query && !r.assets.length)) ?? [];
  return (
    <section className="panel remote-catalog">
      <div className="catalog-toolbar">
        <label>
          {kind === 'game' ? '游戏分支' : '资源项目'}
          <select
            value={selected}
            disabled={busy || !sources.length}
            onChange={(e) => {
              setSelected(e.target.value);
              setQuery('');
              setError('');
              setNotice('');
            }}
          >
            {sources.map((s) => (
              <option value={s.id} key={s.id}>
                {s.name}
              </option>
            ))}
          </select>
        </label>
        <label>
          筛选版本 / 平台附件
          <input
            type="search"
            value={query}
            placeholder="例如 windows、osx、linux、zip"
            onChange={(e) => setQuery(e.target.value)}
          />
        </label>
        <button disabled={busy || !selected} onClick={() => void update()}>
          {busy ? '处理中…' : '更新版本'}
        </button>
      </div>
      {source && (
        <p className="inline-note">
          <a
            href={`https://github.com/${source.repo}/releases`}
            target="_blank"
            rel="noreferrer"
          >
            {source.repo}
          </a>{' '}
          ·{' '}
          {listing?.updated
            ? `同步于 ${new Date(listing.updated).toLocaleString()}`
            : '尚无成功同步'}{' '}
          · 近期 30 个发布及最新正式发布；提交源展示固定提交。之后仅手动更新
        </p>
      )}
      <p className="inline-note">
        下载原始包，不自动解压、执行或替换实例。请按系统与架构选择附件；上游发布标签不代表已验证兼容。
      </p>
      {source?.id === 'kenan' && (
        <p className="inline-note">
          此合集已停更，提供历史测试包；不代表适配最新 DDA。
        </p>
      )}
      {source?.id === 'bnmods' && (
        <p className="inline-note">
          BN
          专用合集，按固定提交下载；解包后逐个登记需要的模组，不自动全部启用。
        </p>
      )}
      {(error || listing?.error) && (
        <p role="alert" className="alert">
          {error || listing?.error}。保留已有目录，可点击更新版本重试。
        </p>
      )}
      {notice && (
        <p role="status" className="file-path">
          {notice}
        </p>
      )}
      {!sources.length && <p>请在桌面应用中浏览和下载上游资源。</p>}
      {selected && !busy && !releases.length && (
        <p>没有可显示的发布附件，请检查筛选或上游发布情况。</p>
      )}
      <div className="release-list">
        {releases.map((r) => (
          <details key={r.tag}>
            <summary>
              <strong>{r.tag}</strong>
              <span>
                {r.assets.some((a) => a.id === 0)
                  ? '固定提交'
                  : r.prerelease
                    ? '上游预发布'
                    : '上游发布'}
              </span>
              <span>{r.published.slice(0, 10)}</span>
              <span>{r.assets.length} 个附件</span>
            </summary>
            <div className="asset-list">
              {r.assets.map((a) => (
                <div className="asset-row" key={a.id}>
                  <span className="file-path">{a.name}</span>
                  <small>{size(a.size)}</small>
                  <small>{a.digest ? '上游 SHA-256' : '仅本地摘要'}</small>
                  <button
                    disabled={busy}
                    onClick={() =>
                      setPending({ source: selected, release: r, asset: a })
                    }
                  >
                    下载
                  </button>
                </div>
              ))}
              {!r.assets.length && <p>此发布没有支持的附件。</p>}
            </div>
          </details>
        ))}
      </div>
      <h3>已下载文件</h3>
      <p className="inline-note">
        解包后，游戏通过“导入本地游戏”登记程序；模组或声音通过“资源来源库”登记单个包目录，再到关联实例配置。下载记录不代表已安装。
      </p>
      <div className="download-list">
        {view?.catalog.downloads
          .filter((d) => sources.some((s) => s.id === d.source))
          .map((d) => (
            <details key={d.id}>
              <summary>
                {d.name} · {size(d.size)} · {d.tag}
              </summary>
              <p className="file-path">{d.path}</p>
              {d.repo && (
                <p className="inline-note">
                  来源：{d.repo} · {d.downloaded_at}
                </p>
              )}
              <p className="file-path">SHA-256：{d.sha256}</p>
              <small>
                {d.upstream_verified
                  ? '下载时与上游摘要匹配'
                  : '本地下载摘要，上游未提供校验值'}
              </small>
              <button disabled={busy} onClick={() => setRemoving(d.id)}>
                删除下载包
              </button>
            </details>
          ))}
      </div>
      {removing && (
        <Modal
          title="删除下载包"
          onClose={() => {
            if (!busy) setRemoving(null);
          }}
        >
          <p>
            仅删除这份已下载的原始包与下载记录；保留已解压文件、资源登记、游戏和实例数据。
          </p>
          {error && <p role="alert">{error}</p>}
          <button disabled={busy} onClick={() => setRemoving(null)}>
            取消
          </button>
          <button
            disabled={busy}
            onClick={() => {
              setBusy(true);
              setError('');
              void client
                .remove(removing)
                .then((v) => {
                  setView(v);
                  setRemoving(null);
                  setNotice('下载包已删除。');
                })
                .catch((e) => setError(message(e)))
                .finally(() => setBusy(false));
            }}
          >
            确认删除下载包
          </button>
        </Modal>
      )}
      {pending && (
        <Modal
          title="确认下载附件"
          onClose={() => {
            if (!busy) setPending(null);
          }}
        >
          <p>
            {sources.find((s) => s.id === pending.source)?.name} ·{' '}
            {pending.release.tag}
          </p>
          <p className="file-path">
            {pending.asset.name} · {size(pending.asset.size)}
          </p>
          <p>保存到系统托管 downloads 目录的独立子目录，不覆盖已有文件。</p>
          {downloading && (
            <p role="status">
              {transfer
                ? `${(transfer.received / 1048576).toFixed(1)} MiB / ${size(transfer.total)}`
                : '正在连接上游…'}
            </p>
          )}
          {downloading && (
            <button
              onClick={() =>
                void client.cancel().catch((e) => setError(message(e)))
              }
            >
              取消下载
            </button>
          )}
          {error && <p role="alert">{error}</p>}
          <button disabled={busy} onClick={() => setPending(null)}>
            取消
          </button>
          <button disabled={busy} onClick={() => void download()}>
            {busy ? '下载并校验中…' : '确认下载'}
          </button>
        </Modal>
      )}
    </section>
  );
}
