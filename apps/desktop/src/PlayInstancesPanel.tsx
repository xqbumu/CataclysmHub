import InstanceResourcesDialog from './InstanceResourcesDialog';
import type { InstanceResourcePreview } from './bindings/InstanceResourcePreview';
import Modal from './Modal';
import type { LaunchPlan } from './api';
import { useState } from 'react';
import { type Dashboard, type HubApi, errorMessage } from './api';

export default function PlayInstancesPanel({
  api,
  data,
  onRefresh,
}: {
  api: HubApi;
  data: Dashboard;
  onRefresh: () => Promise<void>;
}) {
  const [installationId, setInstallationId] = useState('');
  const [name, setName] = useState('');
  const [root, setRoot] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [resourcePreview, setResourcePreview] =
    useState<InstanceResourcePreview | null>(null);
  const [plan, setPlan] = useState<LaunchPlan | null>(null);
  const [profileSelected, setProfileSelected] = useState(false);
  const [profileId, setProfileId] = useState('cdda-0h-source-trial-v1');
  const candidateProfile = [
    'cdda-0h-macos-concurrency-candidate-v1',
    'cdda-0i-1-macos-concurrency-candidate-v1',
  ].includes(profileId);
  async function perform(action: () => Promise<void>) {
    setBusy(true);
    setError('');
    try {
      await action();
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="panel instances-panel">
      <h2>游玩实例 · 隔离准备</h2>
      <p>
        一个安装可以登记多个实例。数据目录用于后续隔离存档、配置、缓存与日志。
      </p>
      <p className="warning">
        实际目录绑定尚未验证。原有启动不使用实例目录；试运行须显式选择参数方案，普通方案并行启动尚未开放；固定
        QA
        组合可在实验构建中选择并发候选，尚未完成真实双开验收。可在“独立备份”选择实例数据根创建尽力快照，安装与共享缓存不在其中。
      </p>
      <label>
        <input
          type="checkbox"
          checked={profileSelected}
          disabled={busy}
          onChange={(event) => setProfileSelected(event.target.checked)}
        />
        使用所选源码参数方案试运行（本地程序版本未验证）
      </label>
      <label>
        试运行参数方案
        <select
          value={profileId}
          disabled={busy}
          onChange={(event) => {
            setProfileId(event.target.value);
            setProfileSelected(false);
          }}
        >
          <option value="cdda-0h-macos-concurrency-candidate-v1">
            CDDA H · 固定 QA 并发候选（实验构建限定）
          </option>
          <option value="cdda-0i-1-macos-concurrency-candidate-v1">
            CDDA I · 固定 QA 并发候选（实验构建限定）
          </option>
          <option value="cdda-0h-source-trial-v1">
            CDDA 0.H 源码参数（通用）
          </option>
          <option value="cdda-0h-release-macos-native-trial-v1">
            CDDA 0.H-RELEASE · macOS 原生本体入口
          </option>
          <option value="cdda-0i-1-macos-native-trial-v1">
            CDDA 0.I-1 · macOS 原生本体入口
          </option>
          <option value="cdda-0h-release-macos-trial-v2">
            CDDA 0.H-RELEASE · macOS · 独立 HOME 与临时目录
          </option>
          <option value="cdda-0i-1-macos-trial-v2">
            CDDA 0.I-1 · macOS · 独立 HOME 与临时目录
          </option>
          <option value="cdda-0h-release-macos-trial-v1">
            CDDA 0.H-RELEASE · macOS SDL2 发布包
          </option>
          <option value="cdda-0i-1-macos-trial-v1">
            CDDA 0.I-1 · macOS SDL2 发布包
          </option>
        </select>
      </label>
      {data.play_instances.map((item) => (
        <article key={item.id}>
          <h3>{item.name} · 绑定未验证</h3>
          <p>
            {
              data.installations.find(
                (game) => game.id === item.installation_id,
              )?.name
            }
          </p>
          <p className="mono">{item.data_root}</p>
          {data.sessions
            .filter((session) => session.play_instance_id === item.id)
            .slice(0, 3)
            .map((session) => (
              <p key={session.id}>
                实例会话：
                {session.state === 'running'
                  ? '运行中'
                  : session.state === 'interrupted_unknown'
                    ? '状态未知，请先确认关闭'
                    : session.state === 'exited_unverified'
                      ? '进程已退出，存档未验证'
                      : session.state}{' '}
                · PID {session.pid ?? '未知'} ·{' '}
                {new Date(session.started_at).toLocaleString('zh-CN')}
              </p>
            ))}
          <button
            className="secondary"
            disabled={
              busy ||
              !api.desktop ||
              !profileSelected ||
              (data.resource_claims.length > 0 && !candidateProfile)
            }
            onClick={() =>
              void perform(async () => {
                setPlan(await api.instanceLaunchPlan(item.id, profileId));
              })
            }
          >
            预览 {item.name} 试运行
          </button>
          <button
            className="secondary"
            disabled={busy || !api.desktop}
            onClick={() =>
              void perform(async () => {
                setResourcePreview(null);
                setResourcePreview(await api.previewInstanceResources(item.id));
              })
            }
          >
            检查 {item.name} 资源冲突
          </button>
        </article>
      ))}
      {error && !plan && <p role="alert">{error}</p>}
      {resourcePreview && (
        <InstanceResourcesDialog
          preview={resourcePreview}
          onClose={() => setResourcePreview(null)}
        />
      )}
      {plan && (
        <Modal
          title="确认实例参数试运行"
          onClose={() => {
            if (!busy) setPlan(null);
          }}
        >
          <dl className="launch-details">
            <dt>游戏程序</dt>
            <dd>{plan.executable}</dd>
            <dt>工作目录（可能写入缓存）</dt>
            <dd>{plan.working_dir}</dd>
            <dt>实例数据目录</dt>
            <dd>{plan.instance_data_root}</dd>
            <dt>目标存档目录（参数指定，绑定未验证）</dt>
            <dd>{plan.save_dir}</dd>
            <dt>进程环境覆盖（仅当前游戏）</dt>
            <dd>{JSON.stringify(plan.environment)}</dd>
            <dt>固定参数</dt>
            <dd>{JSON.stringify(plan.args)}</dd>
            <dt>程序 SHA-256</dt>
            <dd>{plan.executable_sha256}</dd>
          </dl>
          <ul className="plan-warnings">
            {plan.warnings.map((warning) => (
              <li key={warning}>{warning}</li>
            ))}
          </ul>
          {error && <p role="alert">{error}</p>}
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
              disabled={busy}
              onClick={() =>
                void perform(async () => {
                  await api.launch(plan.id);
                  setPlan(null);
                  await onRefresh();
                })
              }
            >
              确认单实例试运行
            </button>
          </div>
        </Modal>
      )}
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void perform(async () => {
            await api.registerPlayInstance({
              installation_id: installationId,
              name,
              data_root: root,
            });
            setName('');
            setRoot('');
            await onRefresh();
          });
        }}
      >
        <label>
          关联安装
          <select
            value={installationId}
            disabled={busy}
            onChange={(e) => setInstallationId(e.target.value)}
          >
            <option value="">选择已导入的游戏</option>
            {data.installations.map((game) => (
              <option key={game.id} value={game.id}>
                {game.name}
              </option>
            ))}
          </select>
        </label>
        <label>
          实例名称
          <input
            value={name}
            disabled={busy}
            maxLength={100}
            onChange={(e) => setName(e.target.value)}
          />
        </label>
        <p>
          选择现有空目录，须与安装、工作区和其他实例分离。登记不会迁移文件。
        </p>
        <button
          type="button"
          className="secondary-button"
          disabled={busy || !api.desktop}
          onClick={() =>
            void perform(async () => {
              const selected = await api.pickPath('directory');
              if (selected) setRoot(selected);
            })
          }
        >
          选择实例空目录
        </button>
        <p className="mono">{root || '尚未选择'}</p>
        <button
          className="primary-button"
          disabled={
            busy || !api.desktop || !installationId || !name.trim() || !root
          }
        >
          登记实例
        </button>
      </form>
    </section>
  );
}
