import RemoteCatalogPanel from './RemoteCatalogPanel';
import WorkspaceTabs from './WorkspaceTabs';
import InstanceContentPanel from './InstanceContentPanel';
import { emptyDashboard, type Dashboard } from './api';
import {
  ContentInstallButton,
  ContentInstallTasks,
} from './ContentInstallWorkflow';
import { useEffect, useState } from 'react';
import type { HubApi } from './api';
import { errorMessage } from './api';
import type { ContentEntry } from './bindings/ContentEntry';
import Modal from './Modal';

export default function ContentPage({
  api,
  data = emptyDashboard,
}: {
  api: HubApi;
  data?: Dashboard;
}) {
  const [entries, setEntries] = useState<ContentEntry[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [filter, setFilter] = useState('all');
  const [query, setQuery] = useState('');
  const [kind, setKind] = useState('mod');
  const [installRevision, setInstallRevision] = useState(0);
  const [removing, setRemoving] = useState<ContentEntry | null>(null);
  useEffect(() => {
    let alive = true;
    if (api.desktop)
      void api
        .contentEntries()
        .then((data) => {
          if (alive) setEntries(data);
        })
        .catch((e: unknown) => {
          if (alive) setError(errorMessage(e));
        });
    return () => {
      alive = false;
    };
  }, [api]);
  async function run(action: () => Promise<unknown>) {
    setBusy(true);
    setError('');
    try {
      await action();
      setEntries(await api.contentEntries());
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  const ids = new Set(
    entries
      .filter((e) => e.kind === 'mod' && !e.last_error)
      .flatMap((e) => e.definitions.map((d) => d.content_id)),
  );
  const counts = new Map<string, number>();
  for (const entry of entries.filter((e) => !e.last_error)) {
    for (const definition of entry.definitions) {
      const key = JSON.stringify([entry.kind, definition.content_id]);
      counts.set(key, (counts.get(key) ?? 0) + 1);
    }
  }
  const visibleEntries = entries.filter(
    (entry) =>
      (filter === 'all' || entry.kind === filter) &&
      (!query ||
        entry.definitions.some((d) =>
          `${d.name} ${d.content_id}`
            .toLowerCase()
            .includes(query.toLowerCase()),
        )),
  );
  const library = (
    <section className="panel">
      <h2>共享资源来源库</h2>
      <p>
        登记本地包目录并读取元数据，登记不复制或启用资源；安装副本需另行预览确认。模组组合属于世界配置；声音包需在游戏设置中选择。所有游戏版本的兼容性均未验证。
      </p>
      <label>
        资源类型{' '}
        <select
          aria-label="资源类型"
          value={kind}
          disabled={busy}
          onChange={(e) => setKind(e.target.value)}
        >
          <option value="mod">模组</option>
          <option value="soundpack">声音包</option>
        </select>
      </label>
      <button
        disabled={busy || !api.desktop}
        onClick={() =>
          void run(async () => {
            const root = await api.pickPath('directory');
            if (root) await api.registerContent(root, kind);
          })
        }
      >
        登记本地包目录
      </button>
      <p className="inline-note">
        选择直接包含 modinfo.json 或 soundpack.txt
        的单个包目录。可重复登记以刷新；源文件保持原样。支持安装到独立资源目录；远端下载见“下载资源”；下载后需解包再登记，不自动启用、升级或删除。
      </p>
      {error && (
        <p role="alert" className="alert">
          {error}
        </p>
      )}
      {!entries.length && <p>尚无登记资源。</p>}
      <div className="content-toolbar">
        <label>
          搜索资源
          <input
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="名称或资源 ID"
          />
        </label>
        <label>
          显示类型
          <select
            value={filter}
            onChange={(event) => setFilter(event.target.value)}
          >
            <option value="all">全部资源</option>
            <option value="mod">模组</option>
            <option value="soundpack">声音包</option>
          </select>
        </label>
      </div>
      {entries.length > 0 && visibleEntries.length === 0 && (
        <p role="status">没有匹配的资源，请调整搜索条件。</p>
      )}
      <div className="content-list">
        {visibleEntries.map((entry) => (
          <article className="panel" key={entry.id}>
            <h3>
              {entry.kind === 'mod' ? '模组' : '声音包'} ·{' '}
              {entry.definitions.map((d) => d.name).join(' / ')}
            </h3>
            <p className="file-path">本地来源：{entry.root}</p>
            <p>兼容性：未验证 · 启用状态：未读取 · 许可证／上游来源：未知</p>
            {entry.last_error && (
              <p role="alert">
                刷新失败：{entry.last_error}
                。下方为历史元数据，不代表资源当前可用。
              </p>
            )}
            {entry.definitions.map((definition, index) => (
              <div key={`${definition.content_id}:${index}`}>
                <p>
                  ID：{definition.content_id} · 版本声明：
                  {definition.version ?? '未知'}
                </p>
                {(counts.get(
                  JSON.stringify([entry.kind, definition.content_id]),
                ) ?? 0) > 1 ? (
                  <p className="alert">
                    重复 ID：请核对不同包与版本，不能据此自动选择覆盖。
                  </p>
                ) : null}
                {entry.kind === 'mod' && (
                  <p>
                    声明依赖：
                    {definition.dependencies === null
                      ? '未知（未声明）'
                      : definition.dependencies.length
                        ? definition.dependencies.join('、')
                        : '空列表'}
                    {definition.dependencies
                      ?.filter((id) => !ids.has(id))
                      .map((id) => (
                        <span key={id}>
                          {' '}
                          · 未在资源库找到：{id}（游戏内置资源未扫描）
                        </span>
                      ))}
                  </p>
                )}
              </div>
            ))}
            <ContentInstallButton
              api={api}
              entryId={entry.id}
              instances={data.play_instances}
              installations={data.installations}
              unavailable={busy || !!entry.last_error}
              onStarted={() => setInstallRevision((r) => r + 1)}
            />
            <details>
              <summary>元数据记录</summary>
              <p>最近成功读取：{entry.checked_at}</p>
              <p className="file-path">
                元数据 SHA-256：{entry.metadata_sha256}
              </p>
              <p>
                仅包含描述文件，不是整包校验，也不证明来源真实性、音频完整或游戏可加载。
              </p>
            </details>
            <button
              disabled={busy || !api.desktop}
              onClick={() => void run(() => api.refreshContent(entry.id))}
            >
              刷新元数据
            </button>
            <button
              disabled={busy || !api.desktop}
              onClick={() => setRemoving(entry)}
            >
              移除登记
            </button>
          </article>
        ))}
      </div>
      <ContentInstallTasks api={api} revision={installRevision} />
      {removing && (
        <Modal
          title="移除资源登记"
          onClose={() => {
            if (!busy) setRemoving(null);
          }}
        >
          <p>
            只从资源库移除 {removing.definitions.map((d) => d.name).join(' / ')}{' '}
            的登记，不删除文件，也不修改世界或声音配置。
          </p>
          <button disabled={busy} onClick={() => setRemoving(null)}>
            取消
          </button>
          <button
            disabled={busy}
            onClick={() =>
              void run(async () => {
                await api.forgetContent(removing.id);
                setRemoving(null);
              })
            }
          >
            确认移除登记
          </button>
        </Modal>
      )}
    </section>
  );
  return (
    <WorkspaceTabs
      label="模组与声音视图"
      items={[
        {
          id: 'instance',
          label: '实例配置',
          content: <InstanceContentPanel api={api} data={data} />,
        },
        {
          id: 'download',
          label: '下载资源',
          content: <RemoteCatalogPanel kind="resource" />,
        },
        { id: 'library', label: '资源来源库', content: library },
      ]}
    />
  );
}
