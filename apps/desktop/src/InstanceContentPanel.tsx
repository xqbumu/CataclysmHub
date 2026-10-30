import { useEffect, useRef, useState } from 'react';
import type { Dashboard, HubApi } from './api';
import { errorMessage } from './api';
import type { ContentConfigReport } from './bindings/ContentConfigReport';
import ContentConfigDialog from './ContentConfigDialog';

export default function InstanceContentPanel({
  api,
  data,
}: {
  api: HubApi;
  data: Dashboard;
}) {
  const [installationId, setInstallationId] = useState('');
  const [instanceId, setInstanceId] = useState('');
  const [report, setReport] = useState<ContentConfigReport | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const request = useRef(0);
  const installation = data.installations.find((i) => i.id === installationId);
  const instance = data.play_instances.find(
    (i) => i.id === instanceId && i.installation_id === installationId,
  );
  useEffect(() => {
    request.current += 1;
    setReport(null);
    setError('');
    setBusy(false);
    return () => {
      request.current += 1;
    };
  }, [
    api,
    installationId,
    instanceId,
    installation?.sha256,
    instance?.data_root,
    instance?.installation_sha256,
  ]);
  async function inspect() {
    if (!instance) return;
    const token = ++request.current;
    setBusy(true);
    setError('');
    setReport(null);
    try {
      const result = await api.inspectContentConfig(instance.id);
      if (request.current !== token) return;
      if (
        result.instance_id !== instance.id ||
        result.data_root !== instance.data_root
      )
        throw new Error('配置报告与目标实例不一致，请重新读取。');
      setReport(result);
    } catch (e) {
      if (request.current === token) setError(errorMessage(e));
    } finally {
      if (request.current === token) setBusy(false);
    }
  }
  return (
    <section className="panel">
      <h2>集中游戏配置</h2>
      <p>
        先选择游戏安装和游玩实例，集中管理语言、自动保存、模组与声音。只修改所选实例；安装内置资源随版本发现，已有世界模组组合单独展示。
      </p>
      <div className="task-center-filters">
        <label>
          目标游戏安装
          <select
            value={installationId}
            onChange={(e) => {
              setInstallationId(e.target.value);
              setInstanceId('');
            }}
          >
            <option value="">选择安装版本</option>
            {data.installations.map((i) => (
              <option key={i.id} value={i.id}>
                {i.name} · {i.version ?? '版本未知'}
              </option>
            ))}
          </select>
        </label>
        <label>
          目标游玩实例
          <select
            value={instance?.id ?? ''}
            disabled={!installation}
            onChange={(e) => setInstanceId(e.target.value)}
          >
            <option value="">选择此安装的实例</option>
            {data.play_instances
              .filter((i) => i.installation_id === installationId)
              .map((i) => (
                <option key={i.id} value={i.id}>
                  {i.name}
                </option>
              ))}
          </select>
        </label>
      </div>
      {!data.installations.length && (
        <p>请先在游戏库导入安装版本并登记实例。</p>
      )}
      {installation &&
        !data.play_instances.some(
          (i) => i.installation_id === installationId,
        ) && <p>此安装尚无实例，请在游戏库登记独立实例。</p>}
      {instance && (
        <p className="file-path">
          当前目标：{instance.name} · {instance.data_root}
        </p>
      )}
      <p className="inline-note">
        资源包版本号不等于游戏兼容范围。来源库登记和独立副本不代表目标已发现或启用；新世界默认模组不会修改已有世界。声音包选择后须重新启动游戏验证。
      </p>
      <button
        className="primary"
        disabled={!instance || busy || !api.desktop}
        onClick={() => void inspect()}
      >
        {busy ? '正在读取目标配置…' : '读取并配置此实例'}
      </button>
      {error && <p role="alert">{error}</p>}
      {report && instance && report.instance_id === instance.id && (
        <ContentConfigDialog
          title={instance.name + ' · 游戏、模组与声音配置'}
          report={report}
          api={api}
          protectionApi={api}
          changeApi={api}
          onClose={() => setReport(null)}
        />
      )}
    </section>
  );
}
