import InstanceProtectionSummary from './InstanceProtectionSummary';
import InstallationQuarantineAction from './InstallationQuarantineAction';
import PathActions from './PathActions';
import type { InstallationRecognition } from './bindings/InstallationRecognition';
import ContentConfigDialog from './ContentConfigDialog';
import type { ContentConfigReport } from './bindings/ContentConfigReport';
import InstanceResourcesDialog from './InstanceResourcesDialog';
import type { InstanceResourcePreview } from './bindings/InstanceResourcePreview';
import Modal from './Modal';
import type { PlayInstance } from './bindings/PlayInstance';
import type { LaunchPlan } from './api';
import { useState } from 'react';
import { type Dashboard, type HubApi, errorMessage } from './api';

export default function PlayInstancesPanel({
  api,
  data,
  onRefresh,
  installationFilter = '',
  home = false,
  focusId = '',
  onOpenActivity,
  onOpenBackups,
  onInstallationFilterChange,
}: {
  api: HubApi;
  data: Dashboard;
  onRefresh: () => Promise<void>;
  installationFilter?: string;
  home?: boolean;
  focusId?: string;
  onOpenActivity?: () => void;
  onOpenBackups?: (instanceId: string) => void;
  onInstallationFilterChange?: (id: string) => void;
}) {
  const [query, setQuery] = useState('');
  const visibleInstances = data.play_instances.filter(
    (item) =>
      (!installationFilter || item.installation_id === installationFilter) &&
      [item.name, item.id, item.data_root]
        .join(' ')
        .toLowerCase()
        .includes(query.trim().toLowerCase()),
  );
  if (home) {
    const lastUsed = (id: string) =>
      Math.max(
        0,
        ...data.sessions
          .filter((s) => s.play_instance_id === id)
          .map((s) => Date.parse(s.started_at) || 0),
      );
    visibleInstances.sort(
      (a, b) =>
        Number(b.id === focusId) - Number(a.id === focusId) ||
        lastUsed(b.id) - lastUsed(a.id) ||
        (Date.parse(b.created_at) || 0) - (Date.parse(a.created_at) || 0),
    );
  }
  const [installationId, setInstallationId] = useState('');
  const [name, setName] = useState('');
  const [directoryMode, setDirectoryMode] = useState('managed');
  const [root, setRoot] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [contentReport, setContentReport] =
    useState<ContentConfigReport | null>(null);
  const [resourcePreview, setResourcePreview] =
    useState<InstanceResourcePreview | null>(null);
  const [plan, setPlan] = useState<LaunchPlan | null>(null);
  const [launchTarget, setLaunchTarget] = useState<PlayInstance | null>(null);
  const [automaticPreview, setAutomaticPreview] = useState(false);
  const [profileSaved, setProfileSaved] = useState(false);
  const [profileSelected, setProfileSelected] = useState(false);
  const [profileId, setProfileId] = useState('');
  const [recognition, setRecognition] =
    useState<InstallationRecognition | null>(null);
  const [recognitionError, setRecognitionError] = useState('');
  const recommendedProfile = recognition?.recommended_profile;
  const candidateProfile = [
    'cdda-0h-macos-concurrency-candidate-v1',
    'cdda-0i-1-macos-concurrency-candidate-v1',
  ].includes(profileId);
  function activeSessionFor(instanceId: string) {
    return data.sessions.find(
      (session) =>
        session.play_instance_id === instanceId &&
        ['starting', 'running', 'interrupted_unknown'].includes(session.state),
    );
  }
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
  async function openSettings(item: PlayInstance, previewRecommended = false) {
    const preferences = await api.preferences();
    setLaunchTarget(item);
    setAutomaticPreview(false);
    setProfileId(preferences.instance_profiles[item.id] ?? '');
    setProfileSelected(false);
    setProfileSaved(false);
    setPlan(null);
    setRecognition(null);
    setRecognitionError('');
    if (api.inspectInstallationRecognition) {
      try {
        const result = await api.inspectInstallationRecognition(
          item.installation_id,
        );
        if (
          result.installation_id !== item.installation_id ||
          result.executable_sha256 !== item.installation_sha256
        ) {
          setRecognitionError('安装记录与实例不一致，请重新核对安装。');
          return;
        }
        setRecognition(result);
        const saved = preferences.instance_profiles[item.id];
        if (
          result.recommended_profile &&
          (!saved || saved === result.recommended_profile)
        ) {
          if (!saved) setProfileId(result.recommended_profile);
          setProfileSelected(true);
          if (previewRecommended) {
            const prepared = await api.instanceLaunchPlan(
              item.id,
              result.recommended_profile,
            );
            setAutomaticPreview(true);
            setPlan(prepared);
          }
        }
      } catch (err) {
        setRecognitionError(errorMessage(err));
      }
    }
  }
  async function startInstance(item: PlayInstance) {
    const preferences = await api.preferences();
    const saved = preferences.instance_profiles[item.id];
    if (!saved || saved.includes('concurrency-candidate')) {
      await openSettings(item, !saved);
      return;
    }
    const prepared = await api.instanceLaunchPlan(item.id, saved);
    await api.launch(prepared.id);
    await onRefresh();
  }
  const launchNotes = (
    <details className="inline-details">
      <summary>启动与目录保护说明</summary>
      <p className="inline-note">
        目录绑定尚未验证。普通方案保持全局互斥；已识别安装自动适配，并发仅限实验候选。
      </p>
    </details>
  );
  return (
    <section
      className={
        home
          ? 'panel instances-panel home-environments'
          : 'panel instances-panel'
      }
    >
      <h2>{home ? '你的游玩环境' : '游玩实例'}</h2>
      {!home && (
        <p>一个安装可以登记多个实例，分别管理存档、配置、缓存与日志。</p>
      )}
      {!home && launchNotes}
      {(!home || data.play_instances.length > 3 || !!query) && (
        <div className="task-center-filters">
          {onInstallationFilterChange && (
            <label>
              筛选安装版本
              <select
                value={installationFilter}
                onChange={(e) => onInstallationFilterChange(e.target.value)}
              >
                <option value="">全部安装版本</option>
                {data.installations.map((game) => (
                  <option key={game.id} value={game.id}>
                    {game.name} · {game.version ?? '版本未知'}
                  </option>
                ))}
                {installationFilter &&
                  !data.installations.some(
                    (game) => game.id === installationFilter,
                  ) && (
                    <option value={installationFilter}>安装记录不可用</option>
                  )}
              </select>
            </label>
          )}
          <label>
            {home ? '搜索游玩环境' : '搜索游玩实例'}
            <input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="实例名称、ID 或数据目录"
            />
          </label>
        </div>
      )}
      {data.play_instances.length > 0 && !visibleInstances.length && (
        <p role="status">没有匹配的游玩实例，请调整版本或搜索条件。</p>
      )}
      {launchTarget && !plan && (
        <Modal
          title={launchTarget.name + ' · 启动设置'}
          onClose={() => {
            if (!busy) setLaunchTarget(null);
          }}
        >
          <p className="file-path">
            目标实例：{launchTarget.name} · {launchTarget.data_root}
          </p>
          <p className="warning">
            已识别的安装会自动选择适配方式。首次确认后记住设置，后续直接启动；实际存档目录仍需在游戏中验收。
          </p>
          {recognition && (
            <p role="status">
              {recognition.version
                ? `识别版本：CDDA ${recognition.version}。`
                : ''}
              {recognition.evidence}
            </p>
          )}
          {recognitionError && (
            <p className="inline-note">{recognitionError}</p>
          )}
          {recognition?.recommended_profile ? (
            profileId === recognition.recommended_profile ? (
              <p>启动适配：自动。无需手动选择游戏版本或参数方案。</p>
            ) : (
              <p>
                此实例保留了手动启动设置。
                <button
                  disabled={busy}
                  onClick={() => {
                    if (!recommendedProfile) return;
                    setProfileId(recommendedProfile);
                    setProfileSelected(true);
                    setProfileSaved(false);
                  }}
                >
                  使用自动适配
                </button>
              </p>
            )
          ) : (
            !busy && (
              <p>
                此安装尚无自动适配，请先核对版本；需要手动试运行时展开高级设置。
              </p>
            )
          )}
          <details className="disclosure">
            <summary>高级启动方案（旧版与实验选项）</summary>
            <label>
              试运行参数方案
              <select
                value={profileId}
                disabled={busy}
                onChange={(event) => {
                  setProfileId(event.target.value);
                  setProfileSelected(false);
                  setProfileSaved(false);
                }}
              >
                <option value="">请选择与此安装匹配的启动方案</option>
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
            <label>
              <input
                type="checkbox"
                checked={profileSelected}
                disabled={busy || !profileId}
                onChange={(event) => setProfileSelected(event.target.checked)}
              />
              使用所选源码参数方案试运行（本地程序版本未验证）
            </label>
            <button
              disabled={busy || !profileId}
              onClick={() =>
                void perform(async () => {
                  await api.saveInstanceProfile(launchTarget.id, profileId);
                  setProfileSaved(true);
                })
              }
            >
              保存此实例启动方案
            </button>
          </details>
          {error && <p role="alert">{error}</p>}
          {profileSaved && (
            <p role="status">
              此实例的启动方案已保存；不代表版本或目录绑定已验证。
            </p>
          )}
          <div className="modal-actions">
            <button
              className="primary"
              disabled={
                busy ||
                !profileId ||
                !profileSelected ||
                !!activeSessionFor(launchTarget.id) ||
                (data.resource_claims.length > 0 && !candidateProfile)
              }
              onClick={() =>
                void perform(async () => {
                  setPlan(
                    await api.instanceLaunchPlan(launchTarget.id, profileId),
                  );
                })
              }
            >
              预览 {launchTarget.name} 试运行
            </button>
          </div>
        </Modal>
      )}
      {!data.play_instances.length && (
        <div className="empty-inline">
          尚无游玩实例。先导入安装版本，再登记独立数据目录。
        </div>
      )}
      {visibleInstances.map((item) => {
        const activeSession = activeSessionFor(item.id);
        const lastSession = data.sessions
          .filter((s) => s.play_instance_id === item.id)
          .sort(
            (a, b) =>
              (Date.parse(b.started_at) || 0) - (Date.parse(a.started_at) || 0),
          )[0];
        const abnormalExit =
          !activeSession &&
          lastSession &&
          (lastSession.state === 'failed' ||
            (lastSession.state === 'exited_unverified' &&
              lastSession.exit_code !== 0));
        const isMacOS = data.installations.some(
          (game) =>
            game.id === item.installation_id && game.platform === 'macos',
        );
        const macOSPending =
          isMacOS &&
          (activeSession?.state === 'starting' ||
            activeSession?.state === 'running');
        const launchLabel = activeSession
          ? activeSession.state === 'starting'
            ? '启动中'
            : activeSession.state === 'running'
              ? '运行中'
              : '待确认关闭'
          : '启动';
        return (
          <article key={item.id}>
            <div className="environment-card-heading">
              <h3>
                {item.name}
                {!home && ' · 绑定未验证'}
              </h3>
              <span className="environment-state">
                {activeSession ? launchLabel : '未监测到运行'}
              </span>
            </div>
            <p>
              {data.installations.find(
                (game) => game.id === item.installation_id,
              )?.name ?? '关联安装记录不可用'}
              {' · '}
              {data.installations.find((g) => g.id === item.installation_id)
                ?.version ?? '版本未知'}
            </p>
            {home && (
              <InstanceProtectionSummary
                api={api}
                data={data}
                instanceId={item.id}
                onOpenBackups={onOpenBackups}
                onOpenActivity={onOpenActivity}
              />
            )}
            <details className="inline-details">
              <summary>数据目录</summary>
              <PathActions api={api} path={item.data_root} />
            </details>
            {!home &&
              data.sessions
                .filter((session) => session.play_instance_id === item.id)
                .slice(0, 3)
                .map((session) => (
                  <p key={session.id}>
                    实例会话：
                    {session.state === 'starting'
                      ? '启动中'
                      : session.state === 'running'
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
            {home && (abnormalExit || macOSPending) && (
              <div
                className={abnormalExit ? 'alert' : 'inline-note'}
                role={abnormalExit ? 'alert' : 'status'}
              >
                <div>
                  {abnormalExit && lastSession ? (
                    <>
                      <strong>上次启动或运行需要检查</strong>
                      <p>
                        {lastSession.exit_code == null
                          ? '未取得正常退出码；请检查系统拦截提示或游戏日志。'
                          : `进程退出码：${lastSession.exit_code}。请检查游戏日志与运行记录。`}
                        此状态不代表存档稳定。
                      </p>
                    </>
                  ) : (
                    <p>
                      当前状态仅反映启动请求或进程状态，不代表已进入游戏。若没有游戏窗口，请检查系统拦截提示；当前继续保护资源，请勿重复启动。
                    </p>
                  )}
                  {isMacOS && (
                    <details className="inline-details">
                      <summary>macOS 启动与授权检查</summary>
                      <ol>
                        <li>
                          关闭系统拦截弹窗，选择“完成”或“好”，不要移到废纸篓。
                        </li>
                        <li>
                          打开“系统设置 →
                          隐私与安全性”，在“安全性”中核对当前游戏名称；仅对你确认来源可信的游戏点击“仍要打开”。
                        </li>
                        <li>
                          亲自完成 Touch ID
                          或管理员验证。若系统另有确认提示，请核对游戏；若明确提示恶意软件，请停止。
                        </li>
                        <li>
                          确认原进程已退出、此环境恢复可启动后，返回这里点击“启动”。若系统自动打开游戏，先正常退出并核对目录。
                        </li>
                      </ol>
                      <p>
                        如果仍没有“仍要打开”，不要反复重试；请保留系统与游戏日志进一步检查。
                        直接双击原 .app
                        可能忽略独立存档参数，请从此游玩环境启动。 Hub
                        不会自动移除隔离属性、更改签名或关闭系统保护。
                      </p>
                      {!activeSession && (
                        <details>
                          <summary>
                            高级排障：隔离属性（绕过首次隔离检查）
                          </summary>
                          <p>
                            优先完成系统正常授权。仅在正常入口无法处理、已核对来源且了解风险时考虑此项；它不是正常授权，也不适用于恶意软件警告。
                          </p>
                          <InstallationQuarantineAction
                            api={api}
                            installationId={item.installation_id}
                            disabled={busy || data.resource_claims.length > 0}
                            onRefresh={onRefresh}
                          />
                        </details>
                      )}
                    </details>
                  )}
                  {onOpenActivity && (
                    <button
                      className="secondary"
                      aria-label={`查看运行记录 ${item.name}`}
                      onClick={onOpenActivity}
                    >
                      查看运行记录
                    </button>
                  )}
                </div>
              </div>
            )}
            {activeSession?.state === 'interrupted_unknown' && (
              <p className="inline-note">
                请先确认游戏已关闭，再到“任务与记录”页面处理状态未知的会话。
              </p>
            )}
            <div className="instance-actions">
              {home && onOpenBackups && (
                <button
                  className="secondary"
                  aria-label={`存档与备份 ${item.name}`}
                  onClick={() => onOpenBackups(item.id)}
                >
                  存档与备份
                </button>
              )}
              <button
                className="primary"
                aria-label={`${launchLabel} ${item.name}`}
                disabled={busy || !api.desktop || !!activeSession}
                onClick={() =>
                  void perform(async () => {
                    await startInstance(item);
                  })
                }
              >
                {launchLabel}
              </button>
              <button
                className={home ? 'secondary' : undefined}
                disabled={busy || !api.desktop}
                aria-label={`启动设置 ${item.name}`}
                onClick={() => void perform(() => openSettings(item))}
              >
                启动设置
              </button>
              <button
                aria-label={`配置 ${item.name}`}
                disabled={busy || !api.desktop}
                onClick={() =>
                  void perform(async () => {
                    setContentReport(null);
                    setContentReport(await api.inspectContentConfig(item.id));
                  })
                }
              >
                配置
              </button>
            </div>
            <details className="inline-details">
              <summary>资源冲突诊断</summary>
              <button
                className="secondary"
                disabled={busy || !api.desktop}
                onClick={() =>
                  void perform(async () => {
                    setResourcePreview(null);
                    setResourcePreview(
                      await api.previewInstanceResources(item.id),
                    );
                  })
                }
              >
                检查 {item.name} 资源冲突
              </button>
            </details>
          </article>
        );
      })}
      {home && launchNotes}
      {error && !plan && !launchTarget && <p role="alert">{error}</p>}
      {contentReport && (
        <ContentConfigDialog
          title={
            (data.play_instances.find((i) => i.id === contentReport.instance_id)
              ?.name ?? contentReport.instance_id) + ' · 模组与声音'
          }
          changeApi={api}
          protectionApi={api}
          api={api}
          report={contentReport}
          onClose={() => setContentReport(null)}
        />
      )}
      {resourcePreview && (
        <InstanceResourcesDialog
          preview={resourcePreview}
          onClose={() => setResourcePreview(null)}
        />
      )}
      {plan && (
        <Modal
          title="启动游戏"
          onClose={() => {
            if (!busy) {
              setPlan(null);
              if (automaticPreview) setLaunchTarget(null);
            }
          }}
        >
          <p>实例：{launchTarget?.name}</p>
          <p className="file-path">存档：{plan.save_dir}</p>
          <details className="disclosure">
            <summary>启动技术详情</summary>
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
          </details>
          <p className="inline-note">
            将向游戏传入此实例的存档与配置目录，请在游戏中核对；进程退出不代表存档稳定。
          </p>
          {candidateProfile && (
            <p className="warning">
              实验并发仅限固定 QA 样本，仍需后端检查完整安装和实例范围。
            </p>
          )}
          {error && <p role="alert">{error}</p>}
          <div className="modal-actions">
            <button
              className="secondary"
              disabled={busy}
              onClick={() => {
                setPlan(null);
                if (automaticPreview) setLaunchTarget(null);
              }}
            >
              取消
            </button>
            <button
              className="primary"
              disabled={busy}
              onClick={() =>
                void perform(async () => {
                  await api.launch(plan.id);
                  if (launchTarget) {
                    try {
                      await api.saveInstanceProfile(launchTarget.id, profileId);
                    } catch {
                      setError(
                        '游戏已启动，但启动设置保存失败；下次需重新设置。',
                      );
                    }
                  }
                  setPlan(null);
                  setLaunchTarget(null);
                  await onRefresh();
                })
              }
            >
              确认启动
            </button>
          </div>
        </Modal>
      )}
      {!home && (
        <details
          className="disclosure"
          open={data.play_instances.length === 0 ? true : undefined}
        >
          <summary>登记新实例</summary>
          <form
            onSubmit={(event) => {
              event.preventDefault();
              void perform(async () => {
                if (directoryMode === 'managed')
                  await api.registerManagedInstance(installationId, name);
                else
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
            <label>
              实例目录方式
              <select
                value={directoryMode}
                disabled={busy}
                onChange={(e) => setDirectoryMode(e.target.value)}
              >
                <option value="managed">
                  系统推荐目录（自动创建独立目录）
                </option>
                <option value="custom">选择空目录或已恢复的实例目录</option>
              </select>
            </label>
            {directoryMode === 'managed' && (
              <p>
                在系统应用数据区的 instances
                下创建唯一目录，与其他实例和安装分离；不会迁移现有存档。
              </p>
            )}
            {directoryMode === 'custom' && (
              <>
                <p>
                  选择空目录，或 Hub 成功恢复的
                  data/。恢复数据会重新校验，须关联原实例的同一游戏安装；登记不会迁移或覆盖文件。
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
                  选择实例目录
                </button>
                <p className="mono">{root || '尚未选择'}</p>
              </>
            )}
            <button
              className="primary-button"
              disabled={
                busy ||
                !api.desktop ||
                !installationId ||
                !name.trim() ||
                (directoryMode === 'custom' && !root)
              }
            >
              登记实例
            </button>
          </form>
        </details>
      )}
    </section>
  );
}
