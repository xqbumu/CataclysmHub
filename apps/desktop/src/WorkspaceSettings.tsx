import { useEffect, useState } from 'react';
import type { HubApi } from './api';
import { errorMessage } from './api';
import type { Workspace } from './bindings/Workspace';
import type { WorkspaceInput } from './bindings/WorkspaceInput';

const fields = [
  ['save_dir', '存档目录', 'saves'],
  ['backup_dir', '独立备份目录', 'backups'],
  ['content_dir', '资源目录', 'content'],
] as const;
const parent = (p: string) => p.replace(/[\\/][^\\/]+[\\/]?$/, '');
const join = (root: string, leaf: string) =>
  `${root.replace(/[\\/]+$/, '')}${root.includes('\\') ? '\\' : '/'}${leaf}`;
export default function WorkspaceSettings({
  api,
  workspace,
  defaults,
  onSaved,
}: {
  api: HubApi;
  workspace: Workspace | null;
  defaults: WorkspaceInput | null;
  onSaved: () => Promise<void>;
}) {
  const [root, setRoot] = useState('');
  const [paths, setPaths] = useState<WorkspaceInput>({
    save_dir: '',
    backup_dir: '',
    content_dir: '',
  });
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState('');
  const [error, setError] = useState('');
  useEffect(() => {
    const v = workspace ?? defaults;
    if (v) {
      setPaths({
        save_dir: v.save_dir,
        backup_dir: v.backup_dir,
        content_dir: v.content_dir,
      });
      setRoot(workspace?.root_dir ?? parent((defaults ?? v).save_dir));
    }
  }, [
    workspace?.id,
    workspace?.root_dir,
    workspace?.save_dir,
    workspace?.backup_dir,
    workspace?.content_dir,
    defaults?.save_dir,
    defaults?.backup_dir,
    defaults?.content_dir,
  ]);
  async function run(fn: () => Promise<void>) {
    setBusy(true);
    setError('');
    try {
      await fn();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="panel setup-panel workspace-settings">
      <h2>{workspace ? '本地工作区' : '建立你的安全工作区'}</h2>
      <p>
        选择一个数据根目录，默认在其下创建
        saves、backups、content；新托管实例位于根目录下的
        instances。可按需单独指定子目录。
      </p>
      <div className="path-field">
        <label htmlFor="workspace-root">数据根目录</label>
        <div className="path-input">
          <input
            id="workspace-root"
            value={root}
            readOnly
            placeholder="选择数据根目录"
          />
          <button
            disabled={busy || !api.desktop}
            onClick={() =>
              void run(async () => {
                const p = await api.pickPath('directory');
                if (p) {
                  setRoot(p);
                  setPaths({
                    save_dir: join(p, 'saves'),
                    backup_dir: join(p, 'backups'),
                    content_dir: join(p, 'content'),
                  });
                }
              })
            }
          >
            选择根目录
          </button>
        </div>
      </div>
      <div className="modal-actions">
        <button
          disabled={!root}
          onClick={() =>
            void run(async () => {
              await navigator.clipboard.writeText(root);
              setMessage('根目录已复制');
            })
          }
        >
          复制根目录
        </button>
        <button
          disabled={
            !api.desktop || !workspace?.root_dir || root !== workspace.root_dir
          }
          onClick={() => void run(() => api.openWorkspacePath(root))}
        >
          打开根目录
        </button>
      </div>
      {defaults && (
        <button
          disabled={busy}
          onClick={() => {
            setRoot(parent(defaults.save_dir));
            setPaths(defaults);
          }}
        >
          恢复系统推荐目录
        </button>
      )}
      <details open={!workspace && !defaults}>
        <summary>自定义子目录与路径操作</summary>
        {fields.map(([key, label, leaf]) => (
          <div className="path-field" key={key}>
            <label htmlFor={key}>{label}</label>
            <div className="path-input">
              <input
                id={key}
                value={paths[key]}
                readOnly
                placeholder="尚未选择目录"
              />
              <button
                disabled={busy || !api.desktop}
                onClick={() =>
                  void run(async () => {
                    const p = await api.pickPath('directory');
                    if (p) setPaths((v) => ({ ...v, [key]: p }));
                  })
                }
              >
                选择
              </button>
              <button
                disabled={busy || !root}
                onClick={() =>
                  setPaths((v) => ({ ...v, [key]: join(root, leaf) }))
                }
              >
                跟随根目录
              </button>
            </div>
            <div className="modal-actions">
              <button
                disabled={!paths[key]}
                onClick={() =>
                  void run(async () => {
                    await navigator.clipboard.writeText(paths[key]);
                    setMessage(`${label}已复制`);
                  })
                }
              >
                复制{label}
              </button>
              <button
                disabled={
                  !workspace || paths[key] !== workspace[key] || !api.desktop
                }
                onClick={() =>
                  void run(() => api.openWorkspacePath(paths[key]))
                }
              >
                打开{label}
              </button>
            </div>
          </div>
        ))}
      </details>
      {workspace && (
        <p>
          修改只影响工作区扫描、资源位置及后续操作，不搬移已有文件；既有实例继续使用原目录。已有备份或任务记录时，请将备份目录保留为原位置。更改根目录后可在上方单独选择原备份目录。
        </p>
      )}
      {workspace && (
        <label>
          <input type="checkbox" required form="workspace-settings" />
          我理解已有数据不会自动迁移，已核对目标目录。
        </label>
      )}
      <form
        id="workspace-settings"
        onSubmit={(e) => {
          e.preventDefault();
          void run(async () => {
            if (!root) {
              await api.initialize(paths);
            } else {
              await api.configureWorkspace(
                root,
                {
                  save_dir: paths.save_dir,
                  backup_dir: paths.backup_dir,
                  content_dir: paths.content_dir,
                },
                workspace
                  ? {
                      save_dir: workspace.save_dir,
                      backup_dir: workspace.backup_dir,
                      content_dir: workspace.content_dir,
                    }
                  : null,
              );
            }
            await onSaved();
            setMessage('工作区已保存，已有文件保持原位置。');
          });
        }}
      >
        <button
          className="primary"
          disabled={
            busy || !api.desktop || Object.values(paths).some((p) => !p)
          }
        >
          {workspace ? '保存目录设置' : '保存工作区'}
        </button>
      </form>
      {error && <p role="alert">{error}</p>}
      {message && <p role="status">{message}</p>}
    </section>
  );
}
