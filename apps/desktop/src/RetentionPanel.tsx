import { useState } from 'react';
import type { HubApi, RetentionPreview } from './api';
const reasons: Record<string, string> = {
  named: '已命名',
  locked: '保留锁定',
  restore_reference: '恢复记录引用',
  not_verified: '未验证或当前不可用',
  future_timestamp: '日期在未来',
  recent: '最近份数',
  latest_verified: '最近一次历史校验通过',
  latest_quiescent: '最近一次历史静默检查',
  daily: '每日窗口代表',
  weekly: '每周窗口代表',
};
export default function RetentionPanel({
  api,
  busy,
  run,
}: {
  api: HubApi;
  busy: boolean;
  run: (action: () => Promise<void>) => Promise<void>;
}) {
  const [recent, setRecent] = useState('7');
  const [days, setDays] = useState('14');
  const [weeks, setWeeks] = useState('8');
  const [report, setReport] = useState<RetentionPreview | null>(null);
  const [page, setPage] = useState(0);
  const values = [recent, days, weeks];
  const invalid =
    values.some((value) => !/^\d{1,4}$/.test(value)) ||
    Number(recent) > 1000 ||
    Number(days) > 366 ||
    Number(weeks) > 104;
  return (
    <section className="panel retention-preview" aria-label="保留规则只读预览">
      <h3>保留规则只读预览</h3>
      <p className="muted">
        仅模拟已登记的手动根目录备份。未开放删除；版本与存档环境关联尚未验证。
      </p>
      <div className="retention-rules">
        <label>
          最近备份份数
          <input
            type="number"
            min={0}
            max={1000}
            value={recent}
            onChange={(event) => setRecent(event.target.value)}
          />
        </label>
        <label>
          每日窗口天数（UTC）
          <input
            type="number"
            min={0}
            max={366}
            value={days}
            onChange={(event) => setDays(event.target.value)}
          />
        </label>
        <label>
          每周窗口周数（UTC）
          <input
            type="number"
            min={0}
            max={104}
            value={weeks}
            onChange={(event) => setWeeks(event.target.value)}
          />
        </label>
        <button
          className="secondary"
          disabled={busy || !api.desktop || invalid}
          onClick={() =>
            void run(async () => {
              setReport(null);
              setPage(0);
              const next = await api.previewRetention({
                keep_recent: Number(recent),
                daily_days: Number(days),
                weekly_weeks: Number(weeks),
              });
              setReport(next);
              setPage(0);
            })
          }
        >
          生成保留预览
        </button>
      </div>
      {invalid && (
        <p className="modal-error" role="alert">
          请输入范围内的非负整数；0 表示停用该条规则。
        </p>
      )}
      {report && (
        <>
          <p>
            按规则保留 {report.protected_count} 份 · 规则外候选{' '}
            {report.candidate_count} 份 / 逻辑数据{' '}
            {report.candidate_logical_bytes} B
          </p>
          <p className="muted">
            生成于 {new Date(report.generated_at).toLocaleString('zh-CN')} ·{' '}
            {report.timezone} · 规则：最近 {report.policy.keep_recent} 份 / 每日{' '}
            {report.policy.daily_days} 天 / 每周 {report.policy.weekly_weeks} 周
          </p>
          <p className="muted">
            这是生成时的快照。修改规则、备份信息或新增备份后，请重新生成；候选不等于允许删除。
          </p>
          <ul>
            {report.warnings.map((warning) => (
              <li key={warning}>{warning}</li>
            ))}
          </ul>
          {report.items.length > 0 && (
            <div className="retention-table">
              <table>
                <thead>
                  <tr>
                    <th>备份</th>
                    <th>创建时间</th>
                    <th>原始数据量</th>
                    <th>规则结果 / 原因</th>
                  </tr>
                </thead>
                <tbody>
                  {report.items
                    .slice(page * 50, (page + 1) * 50)
                    .map((item) => (
                      <tr key={item.backup_id}>
                        <td>
                          {item.display_name || item.backup_id.slice(0, 8)}
                          <small className="mono">{item.backup_id}</small>
                        </td>
                        <td>
                          {new Date(item.created_at).toLocaleString('zh-CN')}
                        </td>
                        <td>{item.total_bytes} B</td>
                        <td>
                          {item.protected ? '保留' : '候选 · 不可据此删除'}
                          {item.reasons.length > 0 && (
                            <small>
                              {item.reasons
                                .map((reason) => reasons[reason] ?? reason)
                                .join('、')}
                            </small>
                          )}
                        </td>
                      </tr>
                    ))}
                </tbody>
              </table>
            </div>
          )}
          {report.items.length > 50 && (
            <div className="modal-actions">
              <button
                className="secondary"
                disabled={page === 0}
                onClick={() => setPage(page - 1)}
              >
                上一页
              </button>
              <span>
                {page + 1} / {Math.ceil(report.items.length / 50)}
              </span>
              <button
                className="secondary"
                disabled={(page + 1) * 50 >= report.items.length}
                onClick={() => setPage(page + 1)}
              >
                下一页
              </button>
            </div>
          )}
        </>
      )}
    </section>
  );
}
