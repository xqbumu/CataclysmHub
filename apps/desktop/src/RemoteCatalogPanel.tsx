import { useEffect, useState } from 'react';
import {
  remoteApi,
  assetHostHint,
  matchesDownload,
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
  onImport,
}: {
  kind: 'game' | 'resource';
  client?: RemoteApi;
  onImport?: (path: string, kind: 'mod' | 'soundpack') => Promise<void> | void;
}) {
  const [view, setView] = useState<RemoteView | null>(null);
  const [selected, setSelected] = useState('');
  const [query, setQuery] = useState('');
  const [tag, setTag] = useState('');
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
    let alive = true;
    let timer: ReturnType<typeof setTimeout>;
    let observed = false;
    async function poll() {
      try {
        const progress = await client.progress();
        if (!alive) return;
        setTransfer(progress);
        if (observed && !progress) {
          const current = await client.view();
          if (alive) setView(current);
        }
        observed = !!progress;
      } catch {
        // Catalog errors remain visible through the existing explicit operations.
      } finally {
        if (alive) timer = setTimeout(() => void poll(), 500);
      }
    }
    void poll();
    return () => {
      alive = false;
      clearTimeout(timer);
    };
  }, [client]);
  const [extracting, setExtracting] = useState<string | null>(null);
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
      setNotice(`下载包已就绪（已有缓存会先校验复用）：${saved.path}`);
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
            disabled={busy || !!transfer || !sources.length}
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
        <button
          disabled={busy || !!transfer || !selected}
          onClick={() => void update()}
        >
          {busy ? '处理中…' : '更新版本'}
        </button>
      </div>
      {transfer && !downloading && (
        <div role="status">
          正在下载：{transfer.name} · {(transfer.received / 1048576).toFixed(1)}{' '}
          MiB / {size(transfer.total)}
          <button
            onClick={() =>
              void client.cancel().catch((e) => setError(message(e)))
            }
          >
            取消下载
          </button>
        </div>
      )}
      {kind === 'game' && (
        <div className="catalog-toolbar">
          <label>
            查找历史发布标签
            <input
              value={tag}
              maxLength={256}
              placeholder="完整 tag，例如 0.H-RELEASE"
              onChange={(e) => setTag(e.target.value)}
            />
          </label>
          <button
            disabled={busy || !!transfer || !tag.trim() || !selected}
            onClick={() =>
              void (async () => {
                setBusy(true);
                setError('');
                try {
                  setView(await client.lookup(selected, tag.trim()));
                  setQuery('');
                  setNotice('已读取该历史发布；不会自动下载。');
                } catch (e) {
                  setError(message(e));
                } finally {
                  setBusy(false);
                }
              })()
            }
          >
            查找历史版本
          </button>
        </div>
      )}
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
          · 近期 30 个发布及最新正式发布；游戏可按完整 tag
          手动查找历史版本，近期更新会重建列表。之后仅手动更新
        </p>
      )}
      <p className="inline-note">
        下载 → 校验缓存 → 独立解压 → 登记游戏或资源 →
        按实例配置。相同附件先校验缓存，缺失或损坏才重新下载；不自动执行或替换实例。
      </p>
      {view?.host_os && (
        <p className="inline-note">
          宿主系统：{view.host_os} · {view.host_arch}
          。附件匹配仅依据文件名，不代表兼容认证。
        </p>
      )}
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
                  <small>
                    {a.digest ? '上游 SHA-256' : '仅本地摘要'}
                    {kind === 'game' && (
                      <>
                        {' '}
                        ·{' '}
                        {assetHostHint(a.name, view?.host_os, view?.host_arch)}
                      </>
                    )}
                  </small>
                  <button
                    disabled={busy || !!transfer}
                    onClick={() =>
                      setPending({ source: selected, release: r, asset: a })
                    }
                  >
                    {source &&
                    view?.catalog.downloads.some((d) =>
                      matchesDownload(d, source, r, a),
                    )
                      ? '校验并复用缓存'
                      : '下载'}
                  </button>
                </div>
              ))}
              {!r.assets.length && <p>此发布没有支持的附件。</p>}
            </div>
          </details>
        ))}
      </div>
      <h3>下载缓存与解压目录</h3>
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
              <button
                onClick={() =>
                  void navigator.clipboard
                    .writeText(d.path)
                    .then(() => setNotice('下载包路径已复制'))
                    .catch((e) => setError(message(e)))
                }
              >
                复制下载包路径
              </button>
              <button
                disabled={
                  busy ||
                  !!transfer ||
                  !/\.(zip|tar|tar\.gz|tgz)$/i.test(d.name)
                }
                onClick={() => setExtracting(d.id)}
              >
                解压到独立目录
              </button>
              {!/\.(zip|tar|tar\.gz|tgz)$/i.test(d.name) && (
                <small>此格式请通过系统工具解包或挂载，再导入。</small>
              )}
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
              <button
                disabled={busy || !!transfer}
                onClick={() => setRemoving(d.id)}
              >
                删除下载包
              </button>
            </details>
          ))}
      </div>
      <h3>已解压目录</h3>
      <p className="inline-note">
        删除下载包后仍保留这些独立目录与记录。目录内容可能被后续导入或手工修改，以下为解压时信息。
      </p>
      {view?.catalog.extractions
        ?.filter((e) => sources.some((s) => s.id === e.source))
        .map((e) => (
          <details key={e.id}>
            <summary>
              {e.name} · {e.tag} · {size(e.bytes)}
            </summary>
            <p className="file-path">{e.path}</p>
            <button
              disabled={busy || !!transfer}
              onClick={() =>
                void client
                  .openExtraction(e.id)
                  .catch((error) => setError(message(error)))
              }
            >
              打开解压目录
            </button>
            {onImport && (
              <button
                disabled={busy || !!transfer}
                onClick={() =>
                  void (async () => {
                    setBusy(true);
                    setError('');
                    try {
                      const path = await client.pickExtraction(
                        e.id,
                        kind === 'game' ? 'executable' : 'directory',
                      );
                      if (path)
                        await onImport(
                          path,
                          sources.find((s) => s.id === e.source)?.kind ===
                            'soundpack'
                            ? 'soundpack'
                            : 'mod',
                        );
                    } catch (error) {
                      setError(message(error));
                    } finally {
                      setBusy(false);
                    }
                  })()
                }
              >
                {kind === 'game' ? '选择并导入游戏' : '选择并登记单个资源包'}
              </button>
            )}
            <small>
              {e.entries} 项 · {e.extracted_at}
            </small>
            <button
              onClick={() =>
                void navigator.clipboard
                  .writeText(e.path)
                  .then(() => setNotice('解压目录已复制'))
                  .catch((error) => setError(message(error)))
              }
            >
              复制解压目录
            </button>
          </details>
        ))}
      {extracting && (
        <Modal
          title="解压下载缓存"
          onClose={() => {
            if (!busy) setExtracting(null);
          }}
        >
          <p>
            先重新校验缓存摘要，再解压到同一缓存容器中的新目录，不覆盖已有游戏、资源或实例。最多
            100000 项、8 GiB；拒绝路径穿越、链接及特殊文件。
          </p>
          <p>
            解压后游戏通过本地导入登记；资源合集需选择单个包登记。解压不代表已安装或已启用。
          </p>
          {error && <p role="alert">{error}</p>}
          <button
            disabled={busy || !!transfer}
            onClick={() => setExtracting(null)}
          >
            取消
          </button>
          <button
            disabled={busy || !!transfer}
            onClick={() => {
              setBusy(true);
              setError('');
              void client
                .extract(extracting)
                .then(async (result) => {
                  setNotice(`已解压：${result.path}`);
                  setExtracting(null);
                  setView(await client.view());
                })
                .catch((e) => setError(message(e)))
                .finally(() => setBusy(false));
            }}
          >
            {busy ? '校验并解压中…' : '确认解压'}
          </button>
        </Modal>
      )}
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
          <button
            disabled={busy || !!transfer}
            onClick={() => setRemoving(null)}
          >
            取消
          </button>
          <button
            disabled={busy || !!transfer}
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
          <p>
            优先校验并复用相同附件的缓存；不可用时下载到系统托管 downloads
            的新子目录，不覆盖已有文件。
          </p>
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
          <button
            disabled={busy || !!transfer}
            onClick={() => setPending(null)}
          >
            取消
          </button>
          <button disabled={busy || !!transfer} onClick={() => void download()}>
            {busy ? '下载并校验中…' : '确认下载'}
          </button>
        </Modal>
      )}
    </section>
  );
}
