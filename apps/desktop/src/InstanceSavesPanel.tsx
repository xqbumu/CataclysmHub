import { useRef, useEffect, useState } from 'react';
import { errorMessage, type Dashboard, type HubApi } from './api';
import type { InstanceSaveReport } from './bindings/InstanceSaveReport';

export default function InstanceSavesPanel({
  api,
  data,
  instanceId,
  busy,
}: {
  api: HubApi;
  data: Dashboard;
  instanceId: string;
  busy: boolean;
}) {
  const instance = data.play_instances.find((i) => i.id === instanceId);
  const [report, setReport] = useState<InstanceSaveReport | null>(null);
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(false);
  const [query, setQuery] = useState('');
  const generation = useRef(0);
  useEffect(
    () => () => {
      generation.current++;
    },
    [],
  );
  const candidates =
    report?.report.candidates.filter((c) =>
      c.relative_root.toLocaleLowerCase().includes(query.toLocaleLowerCase()),
    ) ?? [];
  const blocked = busy || loading || data.resource_claims.length > 0;
  return (
    <section className="panel" aria-label="实例存档">
      <h3>{instance?.name ?? '实例不可用'} · 存档</h3>
      <p className="file-path">
        {instance ? `${instance.data_root}/save` : ''}
      </p>
      <p className="muted">
        只读列出此实例的世界目录候选，不推断角色身份、可读档状态或跨版本兼容性。运行期间不可扫描；外部游戏写入仍可能影响结果。
      </p>
      <button
        disabled={blocked || !instance || !api.inspectInstanceSaves}
        onClick={() => {
          const token = ++generation.current;
          setLoading(true);
          setError('');
          setReport(null);
          void api.inspectInstanceSaves!(instanceId)
            .then((result) => {
              if (token !== generation.current) return;
              if (
                result.instance_id !== instanceId ||
                result.data_root !== instance?.data_root
              )
                throw new Error('扫描来源不匹配，请重新读取。');
              setReport(result);
            })
            .catch((e) => {
              if (token === generation.current) setError(errorMessage(e));
            })
            .finally(() => {
              if (token === generation.current) setLoading(false);
            });
        }}
      >
        {loading ? '正在读取实例存档…' : '读取此实例存档'}
      </button>
      {error && <p role="alert">{error}</p>}
      {report && (
        <>
          <p>
            {report.report.enumeration_complete
              ? '本次目录枚举完成'
              : '本次枚举不完整'}{' '}
            · {report.report.visited_entries} 个条目 ·{' '}
            {report.report.created_at}
          </p>
          <label>
            筛选世界目录
            <input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="世界目录名"
            />
          </label>
          {!report.report.candidates.length && (
            <p>未发现世界目录候选；未创建或修改任何存档。</p>
          )}
          {report.report.candidates.length > 0 && candidates.length === 0 && (
            <p role="status">没有符合筛选的世界目录。</p>
          )}
          {candidates.map((c) => (
            <article className="backup-record" key={c.relative_root}>
              <strong>{c.relative_root}</strong>
              <p>
                {c.file_count} 个普通文件 · {c.total_bytes} B
              </p>
              <small>目录候选 · 格式未知</small>
            </article>
          ))}
          {report.report.issues.map((issue, n) => (
            <p key={n}>
              {issue.code} · {issue.relative_path}
            </p>
          ))}
        </>
      )}
    </section>
  );
}
