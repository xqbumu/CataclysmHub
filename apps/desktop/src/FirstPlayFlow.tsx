import { useState } from 'react';
import type { Dashboard, HubApi } from './api';
import { errorMessage } from './api';
import type { WorkspaceInput } from './bindings/WorkspaceInput';
import type { PlayInstance } from './bindings/PlayInstance';
import WorkspaceSettings from './WorkspaceSettings';
import RemoteCatalogPanel from './RemoteCatalogPanel';

export default function FirstPlayFlow({
  api,
  data,
  defaults,
  mode,
  installationId,
  onSelectInstallation,
  onImport,
  onRefresh,
  onDone,
  onClose,
}: {
  api: HubApi;
  data: Dashboard;
  defaults: WorkspaceInput | null;
  mode: 'download' | 'import';
  installationId: string;
  onSelectInstallation: (id: string) => void;
  onImport: (path?: string) => void;
  onRefresh: () => Promise<void>;
  onDone: (id: string) => void;
  onClose: () => void;
}) {
  const [name, setName] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [created, setCreated] = useState<PlayInstance | null>(null);
  const installation = data.installations.find((i) => i.id === installationId);
  const step = created ? 3 : !data.workspace ? 0 : !installation ? 1 : 2;
  return (
    <section className="first-play" aria-label="首次游玩流程">
      <div className="section-heading">
        <h2>{created ? '环境已创建' : '创建你的游玩环境'}</h2>
        <button className="secondary" disabled={busy} onClick={onClose}>
          返回游玩环境
        </button>
      </div>
      <ol className="flow-steps" aria-label="创建进度">
        {['数据位置', '选择游戏', '创建环境', '准备启动'].map(
          (label, index) => (
            <li key={label} aria-current={step === index ? 'step' : undefined}>
              <span>{index + 1}</span>
              {label}
              {index < step ? ' · 已完成' : ''}
            </li>
          ),
        )}
      </ol>
      {step === 0 && (
        <>
          <p>
            先确定存档、资源和备份的位置。可使用系统推荐目录，之后仍可在设置中调整。
          </p>
          <WorkspaceSettings
            onboarding
            api={api}
            workspace={null}
            defaults={defaults}
            onSaved={onRefresh}
          />
        </>
      )}
      {step === 1 && (
        <div className="panel">
          <h3>{mode === 'download' ? '获取游戏' : '导入已有游戏'}</h3>
          {mode === 'import' ? (
            <>
              <p>
                选择可信的游戏程序或 macOS
                .app，核对名称后导入。原安装和存档保持原位置。
              </p>
              <button
                className="primary"
                disabled={!api.desktop}
                onClick={() => onImport()}
              >
                选择本地游戏
              </button>
            </>
          ) : (
            <>
              <p>
                下载、解压后，点击“选择并导入游戏”，从解压目录选择实际程序。不会自动运行下载内容。
              </p>
              <RemoteCatalogPanel
                kind="game"
                onImport={(path) => onImport(path)}
              />
            </>
          )}
          {data.installations.length > 0 && (
            <div className="existing-game-choice">
              <h3>或使用已导入的版本</h3>
              <label>
                已导入的游戏
                <select
                  value=""
                  onChange={(e) => {
                    if (e.target.value) onSelectInstallation(e.target.value);
                  }}
                >
                  <option value="">选择一个版本继续</option>
                  {data.installations.map((i) => (
                    <option key={i.id} value={i.id}>
                      {i.name} · {i.version ?? '版本未知'}
                    </option>
                  ))}
                </select>
              </label>
            </div>
          )}
        </div>
      )}
      {step === 2 && installation && (
        <form
          className="panel"
          onSubmit={(event) => {
            event.preventDefault();
            if (busy) return;
            setBusy(true);
            setError('');
            void (async () => {
              try {
                const result = await api.registerManagedInstance(
                  installation.id,
                  name.trim() || installation.name,
                );
                setCreated(result);
                await onRefresh();
              } catch (e) {
                setError(errorMessage(e));
              } finally {
                setBusy(false);
              }
            })();
          }}
        >
          <h3>为这次游玩准备独立空间</h3>
          <p>
            游戏：<strong>{installation.name}</strong> ·{' '}
            {installation.version ?? '版本未知'}
          </p>
          <label>
            游玩环境名称
            <input
              value={name}
              placeholder={installation.name}
              maxLength={100}
              disabled={busy}
              onChange={(e) => setName(e.target.value)}
            />
          </label>
          <p className="inline-note">
            默认沿用游戏名称。存档、游戏配置和运行目录会放在当前数据根目录下新建的独立环境中；不会迁移旧存档或覆盖现有环境。
          </p>
          <details>
            <summary>游戏入口</summary>
            <p className="file-path">{installation.executable}</p>
          </details>
          <div className="modal-actions">
            <button
              type="button"
              className="secondary"
              disabled={busy}
              onClick={() => onSelectInstallation('')}
            >
              重新选择游戏
            </button>
            <button className="primary" disabled={busy || !api.desktop}>
              {busy ? '正在创建…' : '创建独立游玩环境'}
            </button>
          </div>
        </form>
      )}
      {created && (
        <div className="panel flow-complete">
          <h3>{created.name}</h3>
          <p>
            独立环境已建立。首次启动仍会检查程序、适配方案和资源占用，并要求确认。
          </p>
          <details>
            <summary>查看独立数据目录</summary>
            <p className="file-path">{created.data_root}</p>
          </details>
          <button
            className="primary"
            disabled={busy}
            onClick={() => {
              setBusy(true);
              setError('');
              void (async () => {
                try {
                  await onRefresh();
                  onDone(created.id);
                } catch (e) {
                  setError(errorMessage(e));
                } finally {
                  setBusy(false);
                }
              })();
            }}
          >
            前往启动
          </button>
          <p className="inline-note">
            原有存档未移动；如需使用旧数据，请先保留保护副本并核对版本。
          </p>
        </div>
      )}
      {error && (
        <p className="alert" role="alert">
          {error}
          {created ? ' 环境已经创建，请返回首页刷新状态；不要重复创建。' : ''}
        </p>
      )}
    </section>
  );
}
