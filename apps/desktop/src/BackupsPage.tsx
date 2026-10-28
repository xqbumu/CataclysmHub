import { useRef, useState } from 'react';
import {
  Archive,
  CircleHelp,
  FolderOpen,
  LoaderCircle,
  RefreshCw,
  ShieldCheck,
  Square,
  TriangleAlert,
} from 'lucide-react';
import Modal from './Modal';
import RetentionPanel from './RetentionPanel';
import BackupDetailsEditor from './BackupDetailsEditor';
import { RestoreButton, RestoreTasks } from './RestoreWorkflow';
import type { BackupPlan, Dashboard, HubApi } from './api';
const phases: Record<string, string> = {
  preflight: '检查范围',
  copying: '复制冻结副本',
  checking_source: '比对源文件 SHA-256',
  archiving: '写入 TAR + Zstandard',
  verifying: '回读校验归档',
  finished: '已结束',
  interrupted: '任务中断',
};
const states: Record<string, string> = {
  running: '执行中',
  cancelling: '正在取消',
  cancelled: '已取消',
  succeeded: '已完成',
  failed: '失败',
  recovery_required: '中断，需要检查',
};
function size(value: string) {
  const bytes = Number(value);
  return bytes < 1024
    ? `${value} B`
    : bytes < 1024 ** 2
      ? `${(bytes / 1024).toFixed(1)} KiB`
      : `${(bytes / 1024 ** 2).toFixed(1)} MiB`;
}
export default function BackupsPage({
  api,
  data,
  busy,
  error,
  run,
  onConfigure,
}: {
  api: HubApi;
  data: Dashboard;
  busy: boolean;
  error: string;
  run: (action: () => Promise<void>) => Promise<void>;
  onConfigure: () => void;
}) {
  const [sourceInstance, setSourceInstance] = useState('');
  const [plan, setPlan] = useState<BackupPlan | null>(null);
  const [acknowledged, setAcknowledged] = useState(false);
  const requestKey = useRef<string | null>(null);
  const active = data.backup_tasks.find((task) =>
    ['running', 'cancelling'].includes(task.state),
  );
  const blocked =
    data.resource_claims.length > 0 ||
    !!active ||
    data.inspection_tasks.some((task) =>
      ['running', 'cancelling'].includes(task.state),
    ) ||
    data.restore_tasks.some((task) =>
      ['running', 'cancelling'].includes(task.state),
    ) ||
    data.scan_tasks.some((task) =>
      ['running', 'cancelling'].includes(task.state),
    ) ||
    data.sessions.some((session) =>
      ['starting', 'running', 'interrupted_unknown'].includes(session.state),
    );
  const latest = data.backup_tasks[0];
  if (!data.workspace)
    return (
      <section className="panel empty-library">
        <FolderOpen size={35} />
        <h2>先配置独立备份目录</h2>
        <p>存档与备份目录必须分离，所有文件操作都在明确选择的范围内进行。</p>
        <button className="primary" onClick={onConfigure}>
          配置工作区
        </button>
      </section>
    );
  return (
    <div className="backups-page">
      <section className="panel scan-toolbar">
        <div>
          <span className="eyebrow">INDEPENDENT FULL ARCHIVES</span>
          <h2>
            {sourceInstance
              ? '实例数据根的完整备份'
              : '整个存档根目录的完整备份'}
          </h2>
          <p className="file-path">{data.workspace.backup_dir}</p>
        </div>
        <label>
          备份来源
          <select
            aria-label="备份来源"
            value={sourceInstance}
            disabled={busy}
            onChange={(event) => {
              setSourceInstance(event.target.value);
              setPlan(null);
              setAcknowledged(false);
              requestKey.current = null;
            }}
          >
            <option value="">工作区存档根目录</option>
            {data.play_instances.map((instance) => (
              <option key={instance.id} value={instance.id}>
                {instance.name} · 实例数据根（绑定未验证）
              </option>
            ))}
          </select>
        </label>
        <button
          className="primary"
          disabled={busy || blocked || !api.desktop}
          onClick={() =>
            void run(async () => {
              const next = await (sourceInstance
                ? api.instanceBackupPlan(sourceInstance)
                : api.backupPlan());
              setPlan(next);
              setAcknowledged(false);
              requestKey.current = crypto.randomUUID();
            })
          }
        >
          <Archive size={16} />
          预览备份范围
        </button>
      </section>
      <div className="notice">
        <CircleHelp size={18} />
        <span>
          按所选工作区存档根或实例数据根创建尽力快照。实例备份不含游戏安装与共享缓存。完整性通过表示文件、清单和
          SHA-256 一致，不代表游戏语义一致或跨版本兼容。覆盖恢复尚未开放。
        </span>
      </div>
      {blocked && !active && (
        <div className="alert">
          <TriangleAlert size={17} />
          工作区正在扫描、恢复或检查目录，或游戏仍在运行/状态未知。请先处理相关任务。
        </div>
      )}
      <RetentionPanel api={api} busy={busy} run={run} />
      {latest && (
        <section className="scan-task" aria-live="polite">
          <div>
            {active ? (
              <LoaderCircle size={16} className="spin" />
            ) : (
              <Archive size={16} />
            )}
            <strong>
              {latest.operation === 'backup.verify' ? '重新校验' : '创建备份'} ·{' '}
              {states[latest.state] ?? latest.state}
            </strong>
            <span>{phases[latest.phase] ?? latest.phase}</span>
            {active && (
              <span>
                {latest.phase === 'verifying'
                  ? '正在回读校验，所需时间取决于容量'
                  : `本阶段已处理 ${latest.processed_entries} 个条目`}
              </span>
            )}
            <span className="mono">{latest.id.slice(0, 8)}</span>
            {latest.error_code && <code>{latest.error_code}</code>}
            {latest.error_code === 'TASK_RESULT_PERSIST_FAILED' && (
              <button
                className="secondary"
                disabled={busy}
                onClick={() =>
                  void run(async () => {
                    await api.retryTaskRecovery(latest.id);
                  })
                }
              >
                重试记录任务状态
              </button>
            )}
          </div>
          {active && (
            <button
              className="secondary"
              disabled={busy || active.state === 'cancelling'}
              onClick={() =>
                void run(async () => {
                  await api.cancelBackup(active.id);
                })
              }
            >
              <Square size={12} />
              取消任务
            </button>
          )}
        </section>
      )}
      {latest?.state === 'recovery_required' && (
        <div className="alert">
          <TriangleAlert size={18} />
          <span>
            上次任务中断。保留的 .partial
            临时目录不会作为有效备份展示；若已发布目录未登记，可用独立校验工具检查。不要手动覆盖原存档。
          </span>
        </div>
      )}
      <RestoreTasks
        api={api}
        tasks={data.restore_tasks}
        inspections={data.inspection_tasks}
        blocked={blocked}
        busy={busy}
        run={run}
      />
      {data.backups.length === 0 ? (
        <section className="panel empty-library">
          <Archive size={36} />
          <h2>建立独立于 Git 的保护副本</h2>
          <p>
            范围确认后将复制原始字节、核对源数据，再生成可独立校验的完整归档。
            <br />
            无法完整读取或出现链接时会中止，不会静默忽略文件。
          </p>
        </section>
      ) : (
        <div className="backup-list">
          {data.backups.map((backup) => (
            <article className="panel backup-record" key={backup.id}>
              <div className="row-between">
                <div>
                  <span className="eyebrow">
                    {backup.play_instance_id
                      ? 'INSTANCE DATA ROOT'
                      : 'SAVE ROOT'}{' '}
                    · TAR / ZSTANDARD
                  </span>
                  <h3>
                    {backup.display_name ||
                      new Date(backup.created_at).toLocaleString('zh-CN')}
                  </h3>
                  {backup.play_instance_id && (
                    <p className="file-path">
                      实例：
                      {data.play_instances.find(
                        (i) => i.id === backup.play_instance_id,
                      )?.name ?? backup.play_instance_id}{' '}
                      · 绑定未验证
                      <br />
                      {backup.source_root}
                    </p>
                  )}
                  {backup.display_name && (
                    <small className="muted">
                      创建于{' '}
                      {new Date(backup.created_at).toLocaleString('zh-CN')}
                    </small>
                  )}
                </div>
                <span
                  className={`pill ${backup.verification === 'verified' ? 'neutral' : 'warning'}`}
                >
                  {backup.verification === 'verified' ? (
                    <ShieldCheck size={14} />
                  ) : (
                    <TriangleAlert size={14} />
                  )}
                  {backup.verification === 'verified'
                    ? '最近校验通过'
                    : backup.verification === 'unavailable'
                      ? '归档当前不可用'
                      : '最近校验失败'}
                </span>
              </div>
              <div className="backup-meta">
                <span>
                  {backup.file_count} 个文件 / {backup.entry_count} 个条目
                </span>
                <span>{size(backup.total_bytes)}</span>
                <span className="pill warning">尽力快照 · 一致性未保证</span>
                {backup.display_name && (
                  <span className="pill neutral">命名保留标记</span>
                )}
                {backup.retention_locked && (
                  <span className="pill neutral">保留已锁定</span>
                )}
              </div>
              <p className="file-path">
                {data.workspace?.backup_dir}/{backup.relative_dir}
              </p>
              {backup.notes && (
                <details>
                  <summary>备注</summary>
                  <p className="backup-notes">{backup.notes}</p>
                </details>
              )}
              <BackupDetailsEditor
                api={api}
                backup={backup}
                busy={busy}
                error={error}
                run={run}
              />
              <details>
                <summary>校验信息与清单</summary>
                <dl className="launch-details">
                  <dt>归档 SHA-256</dt>
                  <dd className="hash">{backup.archive_sha256}</dd>
                  <dt>清单 SHA-256</dt>
                  <dd className="hash">{backup.manifest_sha256}</dd>
                  <dt>上次成功验证时间</dt>
                  <dd>
                    {new Date(backup.verified_at).toLocaleString('zh-CN')}
                  </dd>
                </dl>
              </details>
              <div className="backup-actions">
                <small>
                  保留 archive.tar.zst 与 receipt.json；不依赖原数据库或 Git
                  校验。
                </small>
                <RestoreButton
                  api={api}
                  backupId={backup.id}
                  busy={busy}
                  blocked={blocked}
                  error={error}
                  run={run}
                />
                <button
                  className="secondary"
                  disabled={busy || blocked || !api.desktop}
                  onClick={() =>
                    void run(async () => {
                      await api.verifyBackup(backup.id, crypto.randomUUID());
                    })
                  }
                >
                  <RefreshCw size={14} />
                  重新校验
                </button>
              </div>
            </article>
          ))}
        </div>
      )}
      {plan && (
        <Modal
          title="确认完整备份范围"
          onClose={() => {
            if (!busy) setPlan(null);
          }}
        >
          <p className="muted">
            预览只确认文件范围和容量。确认后才会创建临时副本与归档；源文件不会被移动、覆盖或删除。
          </p>
          <dl className="launch-details">
            <dt>
              {plan.play_instance_id
                ? '源实例数据根（存档、配置及其他普通文件）'
                : '源存档根目录（完整范围）'}
            </dt>
            <dd>{plan.source_root}</dd>
            <dt>目标备份目录</dt>
            <dd>{plan.backup_dir}</dd>
            <dt>范围与容量</dt>
            <dd>
              {plan.file_count} 个文件，{plan.entry_count} 个条目，
              {size(plan.total_bytes)}；预检至少需{' '}
              {size(plan.required_free_bytes)} 可用空间
            </dd>
          </dl>
          <div className="plan-warnings">
            <TriangleAlert size={18} />
            <ul>
              {plan.warnings.map((warning) => (
                <li key={warning}>{warning}</li>
              ))}
            </ul>
          </div>
          <details className="backup-preview">
            <summary>查看将纳入的条目（{plan.entry_count} 项）</summary>
            <ul>
              {plan.entries.slice(0, 200).map((entry) => (
                <li key={entry.relative_path}>
                  <span>{entry.relative_path}</span>
                  <small>
                    {entry.file_type === 'directory'
                      ? '目录'
                      : size(entry.size)}
                  </small>
                </li>
              ))}
            </ul>
            {plan.entries.length > 200 && (
              <p className="muted">
                预览展示前 200
                项；确认范围是整个根目录，所有条目均纳入，不按预览截断。
              </p>
            )}
          </details>
          <label className="backup-ack">
            <input
              type="checkbox"
              checked={acknowledged}
              onChange={(event) => setAcknowledged(event.target.checked)}
            />
            <span>
              我已关闭游戏，理解这是尽力快照，且文件校验不等于游戏一致性保证。
            </span>
          </label>
          {error && (
            <p className="modal-error" role="alert">
              {error}
            </p>
          )}
          <div className="modal-actions">
            <button
              className="secondary"
              disabled={busy}
              onClick={() => setPlan(null)}
            >
              取消
            </button>
            <button
              className="primary"
              disabled={busy || !acknowledged}
              onClick={() =>
                void run(async () => {
                  requestKey.current ??= crypto.randomUUID();
                  await api.createBackup(
                    plan.id,
                    plan.plan_digest,
                    requestKey.current,
                  );
                  setPlan(null);
                })
              }
            >
              {busy && <LoaderCircle size={15} className="spin" />}
              确认创建完整备份
            </button>
          </div>
        </Modal>
      )}
    </div>
  );
}
