import { useEffect, useState } from 'react';
import { errorMessage, type HubApi } from './api';
import type { HubPreferences } from './bindings/HubPreferences';
import Modal from './Modal';

export default function HubSettings({
  api,
  onReset,
}: {
  api: HubApi;
  onReset: () => void;
}) {
  const [preview, setPreview] = useState<HubPreferences | null>(null);
  const [busy, setBusy] = useState(false);
  const [ack, setAck] = useState(false);
  const [error, setError] = useState('');
  const [backup, setBackup] = useState('');
  // A reset preview is fetched on demand, so its revision detects intervening edits.
  useEffect(() => {
    setPreview(null);
  }, [api]);
  async function prepare() {
    setBusy(true);
    setError('');
    setBackup('');
    try {
      setPreview(await api.preferences());
      setAck(false);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="panel">
      <h2>重置 Hub 配置</h2>
      <p>
        将实例启动方案恢复为“未选择”、自动备份策略恢复为默认关闭，并清空本次界面的筛选状态。执行前自动保存
        JSON 配置备份；备份失败不重置。
      </p>
      <p className="inline-note">
        保留已登记工作区路径、游戏安装、实例、资源、备份及恢复保护记录。不会删除文件或修改游戏内配置；已登记目录的迁移不是配置重置。
      </p>
      <button disabled={busy || !api.desktop} onClick={() => void prepare()}>
        预览配置重置
      </button>
      {error && !preview && <p role="alert">{error}</p>}
      {backup && (
        <p role="status" className="file-path">
          配置已重置。重置前备份：{backup}
        </p>
      )}
      {preview && (
        <Modal
          title="备份并重置 Hub 配置"
          onClose={() => {
            if (!busy) setPreview(null);
          }}
        >
          <p>
            将清除 {Object.keys(preview.instance_profiles).length}{' '}
            个实例的已保存启动方案，并重置各实例自动备份与保留规则。以后启动时需重新选择；已开始的备份任务和保护记录不变，旧策略会一同备份。
          </p>
          <details>
            <summary>查看将备份的启动偏好</summary>
            <pre>{JSON.stringify(preview, null, 2)}</pre>
          </details>
          <label>
            <input
              type="checkbox"
              checked={ack}
              onChange={(e) => setAck(e.target.checked)}
              disabled={busy}
            />
            我理解只重置 Hub 启动偏好与界面筛选，保留已登记目录和游戏数据。
          </label>
          {error && <p role="alert">{error}</p>}
          <div className="modal-actions">
            <button disabled={busy} onClick={() => setPreview(null)}>
              取消
            </button>
            <button
              className="primary"
              disabled={busy || !ack}
              onClick={() =>
                void (async () => {
                  setBusy(true);
                  setError('');
                  try {
                    const result = await api.resetPreferences(
                      preview.revision,
                      true,
                    );
                    setBackup(result.backup_path);
                    setPreview(null);
                    onReset();
                  } catch (e) {
                    setError(errorMessage(e));
                  } finally {
                    setBusy(false);
                  }
                })()
              }
            >
              备份并重置
            </button>
          </div>
        </Modal>
      )}
    </section>
  );
}
