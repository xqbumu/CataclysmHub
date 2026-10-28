import { useState } from 'react';
import { type Dashboard, type HubApi, errorMessage } from './api';

export default function PlayInstancesPanel({
  api,
  data,
  onRefresh,
}: {
  api: HubApi;
  data: Dashboard;
  onRefresh: () => Promise<void>;
}) {
  const [installationId, setInstallationId] = useState('');
  const [name, setName] = useState('');
  const [root, setRoot] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  async function perform(action: () => Promise<void>) {
    setBusy(true);
    setError('');
    try {
      await action();
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="panel instances-panel">
      <h2>游玩实例 · 隔离准备</h2>
      <p>
        一个安装可以登记多个实例。数据目录用于后续隔离存档、配置、缓存与日志。
      </p>
      <p className="warning">
        当前仅登记目录，尚未验证游戏实际使用该目录。原有启动不使用实例目录；并行启动尚未开放。
      </p>
      {data.play_instances.map((item) => (
        <article key={item.id}>
          <h3>{item.name} · 绑定未验证</h3>
          <p>
            {
              data.installations.find(
                (game) => game.id === item.installation_id,
              )?.name
            }
          </p>
          <p className="mono">{item.data_root}</p>
        </article>
      ))}
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void perform(async () => {
            await api.registerPlayInstance({
              installation_id: installationId,
              name,
              data_root: root,
            });
            setName('');
            setRoot('');
            await onRefresh();
          });
        }}
      >
        <label>
          关联安装
          <select
            value={installationId}
            disabled={busy}
            onChange={(e) => setInstallationId(e.target.value)}
          >
            <option value="">选择已导入的游戏</option>
            {data.installations.map((game) => (
              <option key={game.id} value={game.id}>
                {game.name}
              </option>
            ))}
          </select>
        </label>
        <label>
          实例名称
          <input
            value={name}
            disabled={busy}
            maxLength={100}
            onChange={(e) => setName(e.target.value)}
          />
        </label>
        <p>
          选择现有空目录，须与安装、工作区和其他实例分离。登记不会迁移文件。
        </p>
        <button
          type="button"
          className="secondary-button"
          disabled={busy || !api.desktop}
          onClick={() =>
            void perform(async () => {
              const selected = await api.pickPath('directory');
              if (selected) setRoot(selected);
            })
          }
        >
          选择实例空目录
        </button>
        <p className="mono">{root || '尚未选择'}</p>
        {error && <p role="alert">{error}</p>}
        <button
          className="primary-button"
          disabled={
            busy || !api.desktop || !installationId || !name.trim() || !root
          }
        >
          登记实例
        </button>
      </form>
    </section>
  );
}
