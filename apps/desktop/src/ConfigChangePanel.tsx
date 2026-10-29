import { useRef, useState } from 'react';
import { errorMessage, type HubApi, type BackupRecord } from './api';
import type { ContentConfigInput } from './bindings/ContentConfigInput';
import type { ConfigChangePlan } from './bindings/ConfigChangePlan';
import type { ConfigChange } from './bindings/ConfigChange';
import type { ConfigChangeObservation } from './bindings/ConfigChangeObservation';
export type ConfigChangeApi = Pick<
  HubApi,
  | 'dashboard'
  | 'prepareConfigChange'
  | 'prepareConfigRollback'
  | 'applyConfigChange'
  | 'configChanges'
  | 'inspectConfigChange'
  | 'resolveConfigChange'
>;
const states: Record<string, string> = {
  applying: '应用执行中或结果待记录',
  applied: '单文件变更已完成',
  recovery_required: '中断或失败，保护仍生效',
  resolved_original: '已确认保留本次变更前配置',
  resolved_proposed: '已确认保留本次变更后配置',
};
export function ConfigChangeHistory({
  api,
}: {
  api: Pick<
    ConfigChangeApi,
    | 'configChanges'
    | 'inspectConfigChange'
    | 'resolveConfigChange'
    | 'prepareConfigRollback'
    | 'applyConfigChange'
  >;
}) {
  const [records, setRecords] = useState<ConfigChange[]>([]);
  const [rollback, setRollback] = useState<ConfigChangePlan | null>(null);
  const [rollbackConfirmed, setRollbackConfirmed] = useState(false);
  const rollbackKey = useRef('');
  const [observation, setObservation] =
    useState<ConfigChangeObservation | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const run = async (action: () => Promise<void>) => {
    setBusy(true);
    setError('');
    try {
      await action();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <section>
      <h4>配置应用记录与中断核对</h4>
      <button
        disabled={busy}
        onClick={() =>
          void run(async () => {
            setRecords(await api.configChanges());
            setObservation(null);
            setRollback(null);
          })
        }
      >
        读取配置应用记录
      </button>
      {records.map((record) => (
        <article key={record.id}>
          <p>
            {record.data_root}/{record.path} ·{' '}
            {states[record.state] ?? record.state}
          </p>
          <p>记录：{record.id}</p>
          {record.rollback_of && <p>回滚来源操作：{record.rollback_of}</p>}
          {record.error_code && <p>{record.error_code}</p>}
          {!record.rollback_of &&
            ['applied', 'resolved_proposed'].includes(record.state) && (
              <button
                disabled={busy}
                onClick={() =>
                  void run(async () => {
                    setRollback(null);
                    setRollbackConfirmed(false);
                    setObservation(null);
                    const next = await api.prepareConfigRollback(record.id);
                    rollbackKey.current = crypto.randomUUID();
                    setRollback(next);
                  })
                }
              >
                预览原字节回滚 {record.id}
              </button>
            )}
          <button
            disabled={busy}
            onClick={() =>
              void run(async () => {
                setObservation(null);
                setObservation(await api.inspectConfigChange(record.id));
              })
            }
          >
            核对当前配置 {record.id}
          </button>
        </article>
      ))}
      {rollback && (
        <div aria-label="原字节回滚确认">
          <p>
            目标：{rollback.data_root}/{rollback.path}
          </p>
          <p>
            原操作：{rollback.rollback_of} · 保护备份：{rollback.backup_id}
          </p>
          <p>当前摘要：{rollback.before_sha256}</p>
          <p>恢复后摘要：{rollback.after_sha256}</p>
          <details>
            <summary>将恢复的原始配置</summary>
            <pre>{rollback.proposed_json}</pre>
          </details>
          <p>
            仅恢复这个文件在保护归档中的原始字节，不修改存档。当前内容若已变化会拒绝执行；原权限、ACL
            等元数据不属于字节回滚承诺。中断时保留保护，不自动重放。
          </p>
          <label>
            <input
              type="checkbox"
              checked={rollbackConfirmed}
              disabled={busy}
              onChange={(e) => setRollbackConfirmed(e.target.checked)}
            />
            我确认恢复上述原配置字节
          </label>
          <button
            disabled={busy || !rollbackConfirmed}
            onClick={() =>
              void run(async () => {
                await api.applyConfigChange(
                  rollback.id,
                  rollback.plan_digest,
                  rollbackKey.current,
                );
                setRollback(null);
                setRecords(await api.configChanges());
              })
            }
          >
            确认原地回滚单文件
          </button>
        </div>
      )}
      {observation && (
        <div aria-label="配置中断核对">
          <p>
            当前文件：
            {observation.matches === 'original'
              ? observation.change.rollback_of
                ? '与本次回滚前的配置一致'
                : '与原配置一致'
              : observation.matches === 'proposed'
                ? observation.change.rollback_of
                  ? '与备份原字节一致'
                  : '与拟应用配置一致'
                : '与原配置和拟应用配置均不同，不能解除保护'}
          </p>
          <code>{observation.current_sha256}</code>
          <p>
            本检查不重放写入、不回滚配置，也不判定原操作成功。确认仅保留当前文件并解除该记录的中断保护。
          </p>
          {['applying', 'recovery_required'].includes(
            observation.change.state,
          ) &&
            observation.matches !== 'other' && (
              <button
                disabled={busy}
                onClick={() =>
                  void run(async () => {
                    const result = await api.resolveConfigChange(
                      observation.change.id,
                      observation.current_sha256,
                    );
                    setObservation(result);
                    setRecords(await api.configChanges());
                  })
                }
              >
                确认保留当前配置并解除中断保护
              </button>
            )}
        </div>
      )}
      {error && <p role="alert">{error}</p>}
    </section>
  );
}
export default function ConfigChangePanel({
  api,
  instanceId,
  input,
}: {
  api: ConfigChangeApi;
  instanceId: string;
  input: ContentConfigInput;
}) {
  const [backups, setBackups] = useState<BackupRecord[]>([]);
  const [backupId, setBackupId] = useState('');
  const [plan, setPlan] = useState<ConfigChangePlan | null>(null);
  const [confirmed, setConfirmed] = useState(false);
  const [record, setRecord] = useState<ConfigChange | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const key = useRef('');
  const run = async (action: () => Promise<void>) => {
    setBusy(true);
    setError('');
    try {
      await action();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <section>
      <h4>应用单个配置文件</h4>
      <p>
        先在独立实验实例使用。默认模组与声音设置须分别应用；已有世界模组不在范围内。所有
        Hub 游戏会话及文件任务结束后才能准备应用，外部程序仍可能改写文件。
      </p>
      {!record && (
        <>
          <button
            disabled={busy}
            onClick={() =>
              void run(async () => {
                setPlan(null);
                setConfirmed(false);
                setBackupId('');
                setBackups(
                  (await api.dashboard()).backups.filter(
                    (b) => b.play_instance_id === instanceId,
                  ),
                );
              })
            }
          >
            读取该实例的保护备份
          </button>
          <label>
            用于恢复原配置的备份
            <select
              disabled={busy}
              value={backupId}
              onChange={(e) => {
                setBackupId(e.target.value);
                setPlan(null);
                setConfirmed(false);
              }}
            >
              <option value="">请选择备份</option>
              {backups.map((b) => (
                <option key={b.id} value={b.id}>
                  {b.display_name || b.id} · {b.created_at}
                </option>
              ))}
            </select>
          </label>
          <button
            disabled={busy || !backupId}
            onClick={() =>
              void run(async () => {
                setPlan(null);
                setConfirmed(false);
                const next = await api.prepareConfigChange(
                  instanceId,
                  input,
                  backupId,
                );
                key.current = crypto.randomUUID();
                setPlan(next);
              })
            }
          >
            校验备份并准备单文件应用
          </button>
        </>
      )}
      {plan && !record && (
        <div aria-label="单文件应用确认">
          <p>
            目标：{plan.data_root}/{plan.path}
          </p>
          <p>保护备份：{plan.backup_id}</p>
          <p>原摘要：{plan.before_sha256}</p>
          <p>新摘要：{plan.after_sha256}</p>
          <pre>{plan.proposed_json}</pre>
          <p>
            此次会替换上述配置文件；保留未修改字段的解析值，JSON
            格式可能变化。不会自动启动游戏或验证加载兼容性。关闭窗口不会取消已经提交的应用。
          </p>
          <label>
            <input
              type="checkbox"
              checked={confirmed}
              disabled={busy}
              onChange={(e) => setConfirmed(e.target.checked)}
            />
            我已核对目标与备份，确认应用这个配置文件
          </label>
          <button
            disabled={busy || !confirmed}
            onClick={() =>
              void run(async () => {
                setRecord(
                  await api.applyConfigChange(
                    plan.id,
                    plan.plan_digest,
                    key.current,
                  ),
                );
              })
            }
          >
            确认应用单文件配置
          </button>
        </div>
      )}
      {record && (
        <p>
          记录 {record.id}：{states[record.state] ?? record.state}
          。请读取下方记录核对最终状态；此响应不代表游戏加载成功。
        </p>
      )}
      {error && <p role="alert">{error}</p>}
      <ConfigChangeHistory api={api} />
    </section>
  );
}
