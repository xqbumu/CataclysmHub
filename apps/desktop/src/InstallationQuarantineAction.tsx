import { useState } from 'react';
import Modal from './Modal';
import { errorMessage, type HubApi } from './api';
import type { InstallationQuarantinePlan } from './bindings/InstallationQuarantinePlan';
import type { InstallationQuarantineResult } from './bindings/InstallationQuarantineResult';

export default function InstallationQuarantineAction({
  api,
  installationId,
  disabled,
  onRefresh,
}: {
  api: HubApi;
  installationId: string;
  disabled: boolean;
  onRefresh: () => Promise<void>;
}) {
  const [plan, setPlan] = useState<InstallationQuarantinePlan | null>(null);
  const [result, setResult] = useState<InstallationQuarantineResult | null>(
    null,
  );
  const [acknowledged, setAcknowledged] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const prepare = api.prepareInstallationQuarantine;
  const apply = api.applyInstallationQuarantine;
  if (!prepare || !apply) return null;
  return (
    <>
      <button
        disabled={disabled || busy || !api.desktop}
        onClick={async () => {
          setBusy(true);
          setError('');
          setResult(null);
          setAcknowledged(false);
          try {
            setPlan(await prepare(installationId));
          } catch (err) {
            setError(errorMessage(err));
          } finally {
            setBusy(false);
          }
        }}
      >
        {busy && !plan ? '正在核对整包范围…' : '检查此安装的隔离属性'}
      </button>
      {error && !plan && <p role="alert">{error}</p>}
      {result && (
        <p role="status">
          已处理 {result.changed_entries}{' '}
          项隔离属性。请从此游玩环境启动；仍须核对游戏实际存档。属性记录：
          <code>{result.backup_directory}</code>
        </p>
      )}
      {plan && (
        <Modal
          title="仅移除此安装的隔离属性"
          onClose={() => {
            if (!busy) setPlan(null);
          }}
        >
          <p>
            这会绕过此安装的首次隔离检查，不属于正常 Gatekeeper
            授权。不会更改游戏字节、签名或全局 Gatekeeper
            设置，也不会自动启动游戏。
          </p>
          <p>
            仅程序摘要匹配已记录样本，不代表整包来源可信；请核对完整下载来源。若系统明确提示恶意软件，请停止，不要使用此操作。
          </p>
          <dl className="launch-details">
            <dt>仅处理此安装（包含普通文件和目录，不跟随链接）</dt>
            <dd>{plan.scope}</dd>
            <dt>待处理隔离属性</dt>
            <dd>{plan.quarantine_entries} 项</dd>
            <dt>属性备份及记录目录（不是存档备份）</dt>
            <dd>{plan.backup_directory}</dd>
          </dl>
          <details>
            <summary>程序核对信息</summary>
            <code>{plan.executable_sha256}</code>
          </details>
          <label className="backup-ack">
            <input
              type="checkbox"
              checked={acknowledged}
              disabled={busy}
              onChange={(e) => setAcknowledged(e.target.checked)}
            />
            我已核对整包来源，理解仅对上述安装移除隔离属性的风险。
          </label>
          {error && <p role="alert">{error}</p>}
          <div className="modal-actions">
            <button disabled={busy} onClick={() => setPlan(null)}>
              取消
            </button>
            <button
              className="primary"
              disabled={busy || disabled || !acknowledged || !!error}
              onClick={async () => {
                setBusy(true);
                setError('');
                try {
                  const completed = await apply(plan.id);
                  setResult(completed);
                  setPlan(null);
                  await onRefresh();
                } catch (err) {
                  setError(errorMessage(err));
                } finally {
                  setBusy(false);
                }
              }}
            >
              {busy ? '正在处理…' : '确认仅移除此安装的隔离属性'}
            </button>
          </div>
        </Modal>
      )}
    </>
  );
}
