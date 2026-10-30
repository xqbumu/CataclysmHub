import { useEffect, useState } from 'react';
import { errorMessage, type HubApi } from './api';
import type { AutomaticBackupPolicy } from './bindings/AutomaticBackupPolicy';
import type { RetentionPreview } from './bindings/RetentionPreview';

export default function AutomaticBackupPolicyPanel({
  api,
  instanceId,
  instanceName = instanceId,
  busy,
  blocked,
  run,
}: {
  api: HubApi;
  instanceId: string;
  instanceName?: string;
  busy: boolean;
  blocked: boolean;
  run: (action: () => Promise<void>) => Promise<void>;
}) {
  const [policy, setPolicy] = useState<AutomaticBackupPolicy | null>(null);
  const [enabled, setEnabled] = useState(false);
  const [minutes, setMinutes] = useState('60');
  const [recent, setRecent] = useState('7');
  const [days, setDays] = useState('14');
  const [weeks, setWeeks] = useState('8');
  const [ack, setAck] = useState(false);
  const [report, setReport] = useState<RetentionPreview | null>(null);
  const [error, setError] = useState('');
  const [retry, setRetry] = useState(0);
  useEffect(() => {
    let current = true;
    setPolicy(null);
    setAck(false);
    setReport(null);
    setError('');
    if (api.automaticBackupPolicy)
      void api
        .automaticBackupPolicy(instanceId)
        .then((next) => {
          if (!current) return;
          if (next.instance_id !== instanceId)
            throw new Error('策略来源与所选实例不符。');
          setPolicy(next);
          setEnabled(next.settings.enabled);
          setMinutes(String(next.settings.min_interval_minutes));
          setRecent(String(next.settings.retention.keep_recent));
          setDays(String(next.settings.retention.daily_days));
          setWeeks(String(next.settings.retention.weekly_weeks));
        })
        .catch((e) => {
          if (current) setError(errorMessage(e));
        });
    return () => {
      current = false;
    };
  }, [api, instanceId, retry]);
  if (
    !api.automaticBackupPolicy ||
    !api.saveAutomaticBackupPolicy ||
    !api.previewInstanceRetention
  )
    return null;
  const retention = {
    keep_recent: Number(recent),
    daily_days: Number(days),
    weekly_weeks: Number(weeks),
  };
  const invalid =
    [minutes, recent, days, weeks].some((v) => !/^\d{1,5}$/.test(v)) ||
    Number(minutes) > 10080 ||
    Number(recent) > 1000 ||
    Number(days) > 366 ||
    Number(weeks) > 104;
  const locked = busy || !policy || !api.desktop;
  const reasonLabels: Record<string, string> = {
    named: '已命名',
    locked: '保留锁',
    restore_reference: '恢复引用',
    not_verified: '未验证/不可用',
    recent: '最近份数',
    daily: '每日窗口',
    weekly: '每周窗口',
    latest_verified: '最近校验通过',
    latest_quiescent: '最近静默检查',
    future_timestamp: '未来日期',
  };
  return (
    <details className="panel disclosure">
      <summary>
        此实例的自动备份与保留策略 ·{' '}
        {policy
          ? policy.settings.enabled
            ? '已开启'
            : '未开启'
          : error
            ? '状态未知（读取失败）'
            : '读取中'}
      </summary>
      <p>
        <strong>目标环境：{instanceName}</strong>
      </p>
      <p>
        按实例保存。当前仅支持已识别的 macOS H/I 原生方案。仅在 Hub
        观察到正常退出、save
        非空且无资源占用时尝试完整备份。游戏运行期间不复制；重启不补做，跳过或失败可查看最近结果。它仍是尽力快照，不保证游戏语义一致。策略不会自动清理任何副本；关闭策略不会中止已开始的备份任务。
      </p>
      <label>
        <input
          type="checkbox"
          checked={enabled}
          disabled={locked}
          onChange={(e) => {
            setEnabled(e.target.checked);
            setAck(false);
          }}
        />
        正常退出后自动备份此实例
      </label>
      <div className="backup-filters">
        <label>
          最小备份间隔（分钟，0 表示每次退出）
          <input
            type="number"
            min={0}
            max={10080}
            value={minutes}
            disabled={locked}
            onChange={(e) => {
              setMinutes(e.target.value);
              setAck(false);
            }}
          />
        </label>
        <label>
          最近保留份数
          <input
            type="number"
            min={0}
            max={1000}
            value={recent}
            disabled={locked}
            onChange={(e) => {
              setRecent(e.target.value);
              setReport(null);
            }}
          />
        </label>
        <label>
          每日窗口天数（UTC）
          <input
            type="number"
            min={0}
            max={366}
            value={days}
            disabled={locked}
            onChange={(e) => {
              setDays(e.target.value);
              setReport(null);
            }}
          />
        </label>
        <label>
          每周窗口周数（UTC）
          <input
            type="number"
            min={0}
            max={104}
            value={weeks}
            disabled={locked}
            onChange={(e) => {
              setWeeks(e.target.value);
              setReport(null);
            }}
          />
        </label>
      </div>
      <p>
        保留规则仅用于此实例的预览。命名、保留锁和恢复引用等保护优先；候选不等于允许删除。最小间隔参考此实例最近记录为校验通过的副本，不能据此保证介质当前有效。
      </p>
      {enabled && (
        <label className="backup-ack">
          <input
            type="checkbox"
            checked={ack}
            disabled={locked}
            onChange={(e) => setAck(e.target.checked)}
          />
          我确认自动备份上述实例，理解完整数据范围、额外磁盘占用及尽力快照限制。
        </label>
      )}
      {invalid && <p role="alert">请输入范围内的非负整数。</p>}
      {error && <p role="alert">{error}</p>}
      <div className="modal-actions">
        <button disabled={busy} onClick={() => setRetry((r) => r + 1)}>
          重新读取策略及最近结果
        </button>
        <button
          disabled={locked || invalid}
          onClick={() =>
            void run(async () => {
              setError('');
              try {
                setReport(null);
                const next = await api.previewInstanceRetention!(
                  instanceId,
                  retention,
                );
                if (next.scope !== `instance_data_root:${instanceId}`)
                  throw new Error('保留预览范围与所选实例不符。');
                setReport(next);
              } catch (e) {
                setError(errorMessage(e));
              }
            })
          }
        >
          预览此实例保留规则
        </button>
        <button
          className="primary"
          disabled={locked || invalid || (enabled && (!ack || blocked))}
          onClick={() =>
            void run(async () => {
              setError('');
              try {
                const saved = await api.saveAutomaticBackupPolicy!(
                  instanceId,
                  policy!.revision,
                  { enabled, min_interval_minutes: Number(minutes), retention },
                  true,
                );
                setPolicy(saved);
                setAck(false);
              } catch (e) {
                setError(errorMessage(e));
              }
            })
          }
        >
          保存此实例策略
        </button>
      </div>
      {blocked && enabled && (
        <p>
          存在活动/未知游戏或文件任务，暂不能开启或修改启用策略；仍可取消勾选后保存关闭。
        </p>
      )}
      {policy && (
        <p role="status">
          最近自动备份：
          {{
            checking: '核对中',
            started: '已创建任务',
            running: '备份中',
            succeeded: '已完成',
            failed: '失败',
            cancelled: '已取消',
            cancelling: '取消中',
            skipped: '已跳过',
            interrupted: '已中断',
            recovery_required: '中断待检查',
          }[policy.last_status ?? ''] ??
            policy.last_status ??
            '尚无记录'}
          {policy.last_error_code
            ? ` · ${{ AUTO_BACKUP_MIN_INTERVAL: '尚未到最小间隔', AUTO_BACKUP_EMPTY_SAVE: 'save 为空，未生成进度副本', AUTO_BACKUP_PROFILE_UNVERIFIED: '启动方案不在支持范围', AUTO_BACKUP_CONTEXT_CHANGED: '实例来源已变化', AUTO_BACKUP_INTERRUPTED: '应用中断，未自动补做' }[policy.last_error_code] ?? '未完成，请查看任务记录'}`
            : ''}
          {policy.last_task_id ? ` · 任务 ${policy.last_task_id}` : ''}
          ；归档结果请在备份列表及任务记录中核对。
        </p>
      )}
      {report && (
        <div>
          <p>
            仅此实例：保留 {report.protected_count} 份 · 规则外候选{' '}
            {report.candidate_count}{' '}
            份（不会删除）。修改策略或备份信息后需重新预览。
          </p>
          <ul>
            {report.items.slice(0, 50).map((item) => (
              <li key={item.backup_id}>
                {item.display_name || item.backup_id} ·{' '}
                {item.protected ? '保留' : '候选'} ·{' '}
                {item.reasons.map((r) => reasonLabels[r] ?? r).join('、')}
              </li>
            ))}
          </ul>
          {report.items.length > 50 && <p>仅展示前50项；计数覆盖完整预览。</p>}
        </div>
      )}
    </details>
  );
}
