import FirstPlayFlow from './FirstPlayFlow';
import InstanceContentPanel from './InstanceContentPanel';
import WorkspaceSettings from './WorkspaceSettings';
import RemoteCatalogPanel from './RemoteCatalogPanel';
import { syncInitialGameCatalog } from './remoteCatalog';
import HubSettings from './HubSettings';
import InstallationDetails from './InstallationDetails';
import TaskCenter from './TaskCenter';
import WorkspaceTabs from './WorkspaceTabs';
import { ConfigChangeHistory } from './ConfigChangePanel';
import ContentPage from './ContentPage';
import InstallationCopiesPanel from './InstallationCopiesPanel';
import ResourceClaimsPanel from './ResourceClaimsPanel';
import PlayInstancesPanel from './PlayInstancesPanel';
import Modal from './Modal';
import BackupsPage from './BackupsPage';
import WorldsPage from './WorldsPage';
import { useCallback, useEffect, useState, type ReactNode } from 'react';
import {
  Activity,
  Archive,
  ArrowRight,
  ChevronRight,
  CircleDot,
  FileText,
  FolderOpen,
  Gamepad2,
  LayoutDashboard,
  LoaderCircle,
  Plus,
  Puzzle,
  Radiation,
  RefreshCw,
  ShieldCheck,
  SlidersHorizontal,
  Terminal,
  TriangleAlert,
  X,
  type LucideIcon,
} from 'lucide-react';
import {
  api as nativeApi,
  emptyDashboard,
  errorMessage,
  parseArguments,
  type HubApi,
  type Installation,
  type LaunchPlan,
  type WorkspaceInput,
} from './api';

type Page =
  'overview' | 'games' | 'backups' | 'content' | 'activity' | 'settings';
const pages: Record<Page, string> = {
  overview: '游玩环境',
  games: '游戏库',
  backups: '存档与备份',
  content: '模组与声音',
  activity: '任务与记录',
  settings: '设置',
};
const statusNames: Record<string, string> = {
  starting: '正在启动',
  running: '运行中',
  exited_unverified: '进程已退出 · 未验证存档',
  failed: '失败',
  interrupted_unknown: '上次运行状态未知',
  resolved_unknown: '已人工确认关闭 · 存档未验证',
  succeeded: '已完成',
  cancelling: '正在取消',
  cancelled: '已取消',
  recovery_required: '操作中断，需重新检查',
};
const actionNames: Record<string, string> = {
  'workspace.initialize': '工作区初始化',
  'workspace.update': '更新工作区目录',
  'installation.import': '导入游戏安装',
  'installation.copy': '复制独立安装',
  'installation.copy_cancel': '取消安装复制',
  'installation.copy_register': '登记已核验副本',
  'instance.register': '登记游玩实例',
  'game.launch': '启动游戏',
  'game.process_result': '记录进程结果',
  'process.reconcile': '重启后状态检查',
  'process.user_acknowledged': '人工确认游戏关闭',
  'world.scan': '只读扫描存档目录',
  'backup.create': '创建独立完整备份',
  'backup.verify': '重新校验备份',
  'backup.restore_new': '恢复到新目录',
  'backup.inspect_restored': '检查已恢复目录',
  'backup.details_update': '编辑备份信息',
  'backup.retention_lock': '锁定备份保留标记',
  'backup.retention_unlock': '解除备份保留标记',
};
function time(value: string) {
  return new Date(value).toLocaleString('zh-CN', {
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  });
}

function NavItem({
  icon: Icon,
  children,
  selected,
  onClick,
  soon,
}: {
  icon: LucideIcon;
  children: ReactNode;
  selected?: boolean;
  onClick?: () => void;
  soon?: boolean;
}) {
  return (
    <button
      className={`nav-item ${selected ? 'selected' : ''}`}
      onClick={onClick}
      disabled={soon}
      aria-current={selected ? 'page' : undefined}
    >
      <Icon size={18} />
      <span>{children}</span>
      {soon && <small>规划中</small>}
    </button>
  );
}

export default function App({ api = nativeApi }: { api?: HubApi }) {
  const [installationDetailsId, setInstallationDetailsId] = useState<
    string | null
  >(null);
  const [installationQuery, setInstallationQuery] = useState('');
  const [settingsRevision, setSettingsRevision] = useState(0);
  const [defaultPaths, setDefaultPaths] = useState<WorkspaceInput | null>(null);
  const [instanceFilter, setInstanceFilter] = useState('');
  const [backupView, setBackupView] = useState('archives');
  const [gameView, setGameView] = useState('instances');
  const [page, setPage] = useState<Page>('overview');
  const [firstPlay, setFirstPlay] = useState<'download' | 'import' | null>(
    null,
  );
  const [firstInstallationId, setFirstInstallationId] = useState('');
  const [homeFocusId, setHomeFocusId] = useState('');
  const [data, setData] = useState(emptyDashboard);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [importing, setImporting] = useState(false);
  const [importDestination, setImportDestination] = useState<
    'library' | 'first-play'
  >('library');
  const [name, setName] = useState('');
  const [executable, setExecutable] = useState('');
  const [launching, setLaunching] = useState<Installation | null>(null);
  const [argsText, setArgsText] = useState('[]');
  const [plan, setPlan] = useState<LaunchPlan | null>(null);
  const [acknowledging, setAcknowledging] = useState<string | null>(null);
  const refresh = useCallback(async () => {
    const result = await api.dashboard();
    setData(result);
  }, [api]);
  useEffect(() => {
    void syncInitialGameCatalog();
  }, []);
  useEffect(() => {
    let stopped = false;
    let timeout: ReturnType<typeof setTimeout>;
    async function poll() {
      try {
        const result = await api.dashboard();
        if (!stopped) setData(result);
      } catch (err) {
        if (!stopped) setError(errorMessage(err));
      } finally {
        if (!stopped) {
          setLoading(false);
          timeout = setTimeout(poll, 2000);
        }
      }
    }
    void poll();
    return () => {
      stopped = true;
      clearTimeout(timeout);
    };
  }, [api]);
  async function act(action: () => Promise<void>) {
    if (busy) return;
    setBusy(true);
    setError('');
    setNotice('');
    try {
      await action();
      await refresh();
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }
  useEffect(() => {
    let alive = true;
    if (api.desktop)
      void api
        .workspaceDefaults()
        .then((defaults) => {
          if (!alive || !defaults) return;
          setDefaultPaths(defaults);
        })
        .catch((err) => {
          if (alive) setError(errorMessage(err));
        });
    return () => {
      alive = false;
    };
  }, [api]);
  const workspace = data.workspace;
  const unknown = data.sessions.filter(
    (s) => s.state === 'interrupted_unknown',
  );
  const closeLaunch = () => {
    if (!busy) {
      setLaunching(null);
      setPlan(null);
    }
  };
  function openImport() {
    setImportDestination('library');
    setName('');
    setExecutable('');
    setImporting(true);
  }
  function openLaunch(game: Installation) {
    setLaunching(game);
    setPlan(null);
    setArgsText('[]');
  }

  const setup = (
    <WorkspaceSettings
      api={api}
      workspace={workspace}
      defaults={defaultPaths}
      onSaved={refresh}
    />
  );

  const detailedInstallation = data.installations.find(
    (game) => game.id === installationDetailsId,
  );
  const filteredInstallations = data.installations.filter((game) =>
    [game.name, game.id, game.version ?? '', game.branch ?? '', game.executable]
      .join(' ')
      .toLowerCase()
      .includes(installationQuery.trim().toLowerCase()),
  );
  const gameCards = (
    <div className="game-grid">
      {filteredInstallations.map((game) => {
        const sessions = data.sessions.filter(
          (s) => s.installation_id === game.id,
        );
        const active =
          sessions.find((s) =>
            ['running', 'starting', 'interrupted_unknown'].includes(s.state),
          ) ??
          [...sessions].sort((a, b) =>
            b.started_at.localeCompare(a.started_at),
          )[0];
        const blocked =
          data.resource_claims.length > 0 ||
          data.sessions.some((s) =>
            ['running', 'starting', 'interrupted_unknown'].includes(s.state),
          ) ||
          data.scan_tasks.some((task) =>
            ['running', 'cancelling'].includes(task.state),
          ) ||
          data.inspection_tasks.some((task) =>
            ['running', 'cancelling'].includes(task.state),
          ) ||
          data.restore_tasks.some((task) =>
            ['running', 'cancelling'].includes(task.state),
          ) ||
          data.backup_tasks.some((task) =>
            ['running', 'cancelling'].includes(task.state),
          );
        return (
          <article className="panel game-card" key={game.id}>
            <div className="game-details">
              <div className="row-between">
                <span className="eyebrow">
                  {game.platform} / {game.arch}
                </span>
                <span className="pill neutral">未验证</span>
              </div>
              <h3>{game.name}</h3>
              <p className="muted">
                登记版本 {game.version ?? '未知'} · 分支 {game.branch ?? '未知'}
              </p>
              <p className="file-path" title={game.executable}>
                <Terminal size={13} />
                {game.executable}
              </p>
              {active && (
                <div
                  className={`session-state ${active.state === 'running' ? 'green' : ''}`}
                >
                  <CircleDot size={13} />
                  {active.state === 'failed'
                    ? '启动失败'
                    : (statusNames[active.state] ?? active.state)}
                </div>
              )}
              <button
                className="secondary full"
                onClick={() => setInstallationDetailsId(game.id)}
              >
                查看 {game.name} 关联详情
              </button>
              <button
                className="primary full"
                disabled={busy || blocked}
                onClick={() => openLaunch(game)}
              >
                <Gamepad2 size={16} />
                {blocked ? '工作区忙碌或待确认' : '启动预览'}
                <ChevronRight size={16} />
              </button>
            </div>
          </article>
        );
      })}
    </div>
  );

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <a
          className="brand"
          href="#"
          onClick={(event) => {
            event.preventDefault();
            setPage('overview');
          }}
        >
          <span className="brand-mark">
            <Radiation size={24} />
          </span>
          <div>
            CATACLYSM
            <span>
              HUB <i> / </i> 生存者工作台
            </span>
          </div>
        </a>
        <div className="workspace-label">
          <span className="status-dot" />
          本地工作区<small>LOCAL</small>
        </div>
        <nav aria-label="主导航">
          <span className="nav-label">工作台</span>
          <NavItem
            icon={LayoutDashboard}
            selected={page === 'overview'}
            onClick={() => setPage('overview')}
          >
            游玩环境
          </NavItem>
          <NavItem
            icon={Gamepad2}
            selected={page === 'games'}
            onClick={() => setPage('games')}
          >
            游戏库<span className="nav-count">{data.installations.length}</span>
          </NavItem>
          <NavItem
            icon={Archive}
            selected={page === 'backups'}
            onClick={() => setPage('backups')}
          >
            存档与备份
          </NavItem>
          <span className="nav-label second">管理</span>
          <NavItem
            icon={Puzzle}
            selected={page === 'content'}
            onClick={() => setPage('content')}
          >
            模组与声音
          </NavItem>
          <NavItem
            icon={FileText}
            selected={page === 'activity'}
            onClick={() => setPage('activity')}
          >
            任务与记录
          </NavItem>
          <NavItem
            icon={SlidersHorizontal}
            selected={page === 'settings'}
            onClick={() => setPage('settings')}
          >
            设置
          </NavItem>
        </nav>
        <div className="sidebar-bottom">
          <div className="local-note">
            <ShieldCheck size={17} />
            <div>
              本地优先<span>你的世界，留在你的设备上。</span>
            </div>
          </div>
          <div className="build-label">
            <span>v0.1.0</span>
            <span>
              开发预览 <span className="status-dot amber" />
            </span>
          </div>
        </div>
      </aside>
      <div className="main-shell">
        <header className="topbar">
          <div className="breadcrumbs">
            工作台
            <ChevronRight size={13} />
            <span>{pages[page]}</span>
          </div>
          <div className="topbar-right">
            <span className="offline">
              <span className="status-dot" />
              离线可用
            </span>
            <button
              className="icon-button"
              onClick={() => void act(refresh)}
              disabled={busy || loading}
              aria-label="刷新状态"
            >
              <RefreshCw size={16} className={busy ? 'spin' : ''} />
            </button>
          </div>
        </header>
        <main>
          <div className="page-heading">
            <div>
              <h1>{pages[page]}</h1>
              <p>
                {page === 'overview'
                  ? '找到你的游玩环境，启动游戏，或准备一次新的出发。'
                  : page === 'games'
                    ? '实例用于游玩，安装版本用于管理游戏程序。'
                    : page === 'backups'
                      ? '查看世界、创建保护副本，或恢复到新目录。'
                      : page === 'content'
                        ? '按安装与实例配置模组和声音，来源资源单独管理。'
                        : page === 'activity'
                          ? '集中处理资源占用、配置恢复与操作记录。'
                          : '查看本机数据位置与工作区配置。'}
              </p>
            </div>
            {page === 'overview' &&
              data.play_instances.length > 0 &&
              !firstPlay && (
                <button
                  className="secondary"
                  onClick={() => {
                    setFirstInstallationId('');
                    setFirstPlay('import');
                  }}
                >
                  添加游玩环境
                </button>
              )}
            {page === 'games' && (
              <button
                className="primary"
                disabled={!workspace || busy || !api.desktop}
                onClick={openImport}
              >
                <Plus size={17} />
                导入游戏
              </button>
            )}
          </div>
          {!api.desktop && (
            <div className="notice">
              <Terminal size={17} />
              <span>
                浏览器界面预览 · 本地文件与启动功能请运行{' '}
                <code>npm run desktop dev</code>
              </span>
            </div>
          )}
          {error && (
            <div className="alert" role="alert">
              <TriangleAlert size={18} />
              <span>{error}</span>
              <button
                className="icon-button"
                aria-label="关闭错误提示"
                onClick={() => setError('')}
              >
                <X size={16} />
              </button>
            </div>
          )}
          {notice && (
            <div className="notice" role="status">
              <ShieldCheck size={17} />
              {notice}
            </div>
          )}
          {unknown.map((session) => (
            <div className="alert" key={session.id}>
              <TriangleAlert size={18} />
              <div>
                上次运行状态未知。请检查游戏是否仍在运行；不会将该会话标记为存档稳定。
              </div>
              <button
                className="secondary"
                disabled={busy}
                onClick={() => setAcknowledging(session.id)}
              >
                检查后确认
              </button>
            </div>
          ))}
          {loading ? (
            <div className="loading" role="status">
              <LoaderCircle className="spin" />
              正在读取本地工作区…
            </div>
          ) : (
            <>
              {(page === 'overview' || !!firstPlay) && (
                <div hidden={page !== 'overview'}>
                  {firstPlay ? (
                    <FirstPlayFlow
                      api={api}
                      data={data}
                      defaults={defaultPaths}
                      mode={firstPlay}
                      installationId={firstInstallationId}
                      onSelectInstallation={setFirstInstallationId}
                      onRefresh={refresh}
                      onImport={(path) => {
                        if (path) {
                          setExecutable(path);
                          setName('');
                          setImporting(true);
                        } else openImport();
                        setImportDestination('first-play');
                      }}
                      onClose={() => {
                        setFirstPlay(null);
                        setFirstInstallationId('');
                      }}
                      onDone={(id) => {
                        setNotice('游玩环境已创建。首次启动请核对方案并确认。');
                        setHomeFocusId(id);
                        setFirstPlay(null);
                        setFirstInstallationId('');
                      }}
                    />
                  ) : data.play_instances.length > 0 ? (
                    <PlayInstancesPanel
                      key={settingsRevision}
                      api={api}
                      data={data}
                      onRefresh={refresh}
                      home
                      focusId={homeFocusId}
                    />
                  ) : (
                    <section className="panel play-welcome">
                      <span className="eyebrow">CATACLYSM HUB</span>
                      <h2>准备你的第一次游玩</h2>
                      <p>选一个游戏版本，建立独立存档与配置，然后开始游戏。</p>
                      <div className="welcome-choices">
                        <button
                          className="primary"
                          onClick={() => {
                            setFirstInstallationId('');
                            setFirstPlay('download');
                          }}
                        >
                          <Gamepad2 />
                          获取游戏<span>从官方发布选择版本</span>
                        </button>
                        <button
                          className="secondary"
                          onClick={() => {
                            setFirstInstallationId('');
                            setFirstPlay('import');
                          }}
                        >
                          <FolderOpen />
                          导入已有游戏<span>保留原安装与存档</span>
                        </button>
                      </div>
                      {data.installations.length > 0 && (
                        <button
                          className="text-button"
                          onClick={() => {
                            setFirstInstallationId('');
                            setFirstPlay('import');
                          }}
                        >
                          使用已导入的版本创建环境
                        </button>
                      )}
                      <p className="inline-note">
                        每个环境使用独立数据目录。备份与恢复仍需明确选择范围和确认。
                      </p>
                    </section>
                  )}
                  {data.resource_claims.length > 0 && (
                    <ResourceClaimsPanel
                      claims={data.resource_claims}
                      onAcknowledge={setAcknowledging}
                      busy={busy}
                    />
                  )}
                  {!firstPlay && data.play_instances.length > 0 && (
                    <div className="home-utilities">
                      <button
                        className="secondary"
                        onClick={() => setPage('backups')}
                      >
                        <Archive />
                        存档与保护副本
                      </button>
                      <button
                        className="text-button"
                        onClick={() => setPage('activity')}
                      >
                        <FileText />
                        任务与运行记录
                      </button>
                    </div>
                  )}
                </div>
              )}
              {page === 'backups' && (
                <WorkspaceTabs
                  label="存档管理视图"
                  value={backupView}
                  onChange={setBackupView}
                  items={[
                    {
                      id: 'archives',
                      label: '独立备份',
                      content: (
                        <BackupsPage
                          api={api}
                          data={data}
                          busy={busy}
                          error={error}
                          run={act}
                          onConfigure={() => setPage('settings')}
                        />
                      ),
                    },
                    {
                      id: 'worlds',
                      label: '世界与角色',
                      content: (
                        <WorldsPage
                          api={api}
                          data={data}
                          busy={busy}
                          run={act}
                          onConfigure={() => setPage('settings')}
                        />
                      ),
                    },
                  ]}
                />
              )}
              {page === 'content' && <ContentPage api={api} data={data} />}
              {page === 'settings' && (
                <div className="settings-layout">
                  {setup}
                  <InstanceContentPanel api={api} data={data} />
                  <HubSettings
                    api={api}
                    onReset={() => {
                      setSettingsRevision((r) => r + 1);
                      setGameView('instances');
                      setBackupView('archives');
                      setInstanceFilter('');
                      setInstallationQuery('');
                    }}
                  />
                </div>
              )}
              {page === 'games' && (
                <WorkspaceTabs
                  label="游戏库视图"
                  value={gameView}
                  onChange={setGameView}
                  items={[
                    {
                      id: 'instances',
                      label: '游玩实例',
                      content: workspace ? (
                        <PlayInstancesPanel
                          key={settingsRevision}
                          api={api}
                          data={data}
                          installationFilter={instanceFilter}
                          onInstallationFilterChange={setInstanceFilter}
                          onRefresh={refresh}
                        />
                      ) : (
                        <EmptyLibrary
                          disabled
                          onImport={openImport}
                          needsWorkspace
                          onSetup={() => setPage('settings')}
                        />
                      ),
                    },
                    {
                      id: 'installations',
                      label: `安装版本 (${data.installations.length})`,
                      content: data.installations.length ? (
                        <>
                          <label>
                            搜索安装版本
                            <input
                              value={installationQuery}
                              onChange={(e) =>
                                setInstallationQuery(e.target.value)
                              }
                              placeholder="名称、版本、分支或程序路径"
                            />
                          </label>
                          {!filteredInstallations.length && (
                            <p role="status">没有匹配的安装版本。</p>
                          )}
                          {gameCards}
                        </>
                      ) : (
                        <EmptyLibrary
                          disabled={!workspace || busy || !api.desktop}
                          onImport={openImport}
                          needsWorkspace={!workspace}
                          onSetup={() => setPage('settings')}
                        />
                      ),
                    },
                    {
                      id: 'remote',
                      label: '获取游戏',
                      content: (
                        <RemoteCatalogPanel
                          kind="game"
                          onImport={(path) => {
                            setImportDestination('library');
                            setExecutable(path);
                            setName('');
                            setImporting(true);
                          }}
                        />
                      ),
                    },
                    {
                      id: 'copies',
                      label: '安装复制',
                      content: workspace ? (
                        <InstallationCopiesPanel
                          api={api}
                          data={data}
                          onRefresh={refresh}
                        />
                      ) : (
                        <p className="notice">请先配置工作区。</p>
                      ),
                    },
                  ]}
                />
              )}
              {page === 'activity' && (
                <>
                  <TaskCenter
                    data={data}
                    api={api}
                    onOpen={(destination) => {
                      if (destination === 'copies') {
                        setGameView('copies');
                        setPage('games');
                      } else if (destination === 'content') setPage('content');
                      else {
                        setBackupView(destination);
                        setPage('backups');
                      }
                    }}
                  />
                  <ResourceClaimsPanel
                    claims={data.resource_claims}
                    onAcknowledge={setAcknowledging}
                    busy={busy}
                  />
                  <details className="panel disclosure">
                    <summary>配置应用与回滚记录</summary>
                    <ConfigChangeHistory api={api} />
                  </details>
                  <section className="panel activity-panel">
                    <div className="section-heading">
                      <h2>本地操作日志</h2>
                      <span className="muted">
                        最近 {data.operations.length} 条 · 不含存档内容
                      </span>
                    </div>
                    {data.operations.length ? (
                      <div className="table-scroll">
                        <table>
                          <thead>
                            <tr>
                              <th>操作</th>
                              <th>状态</th>
                              <th>任务标识</th>
                              <th>时间</th>
                            </tr>
                          </thead>
                          <tbody>
                            {data.operations.map((op) => (
                              <tr key={op.id}>
                                <td>
                                  {actionNames[op.action] ?? op.action}
                                  {op.error_code && (
                                    <small className="error-code">
                                      {op.error_code}
                                    </small>
                                  )}
                                </td>
                                <td>{statusNames[op.status] ?? op.status}</td>
                                <td className="mono">
                                  {op.task_id.slice(0, 8)}
                                </td>
                                <td>{time(op.created_at)}</td>
                              </tr>
                            ))}
                          </tbody>
                        </table>
                      </div>
                    ) : (
                      <div className="empty-inline">
                        <FileText size={22} />
                        <span>尚无本地记录</span>
                      </div>
                    )}
                  </section>
                </>
              )}
            </>
          )}
          <footer>
            <span>
              <CircleDot size={12} />
              本地数据 · 显式操作 · 可追溯记录
            </span>
            <span>
              CATACLYSM HUB <i> / </i> DEVELOPMENT PREVIEW
            </span>
          </footer>
        </main>
      </div>
      {importing && (
        <Modal
          title="导入现有游戏"
          onClose={() => {
            if (!busy) setImporting(false);
          }}
        >
          <form
            onSubmit={(event) => {
              event.preventDefault();
              void act(async () => {
                const imported = await api.importInstallation(executable, name);
                setImporting(false);
                if (firstPlay && importDestination === 'first-play') {
                  setFirstInstallationId(imported.id);
                  setPage('overview');
                } else {
                  setPage('games');
                  setGameView('installations');
                }
                setNotice(
                  firstPlay && importDestination === 'first-play'
                    ? '游戏已导入，接下来创建独立游玩环境。'
                    : '安装已导入。可在安装卡查看登记版本，首次启动时会检查当前程序并推荐适用方案。',
                );
              });
            }}
          >
            <p className="muted">
              选择你信任的游戏程序或 macOS .app
              应用。这里只建立本地索引，不复制游戏。
            </p>
            <label className="field-label" htmlFor="install-name">
              安装名称
            </label>
            <input
              id="install-name"
              autoFocus
              value={name}
              onChange={(event) => setName(event.target.value)}
              maxLength={100}
              placeholder="例如：CDDA · 本地安装"
              required
            />
            <label className="field-label" htmlFor="executable">
              游戏可执行文件
            </label>
            <div className="path-input">
              <Terminal size={16} />
              <input
                id="executable"
                readOnly
                value={executable}
                placeholder="选择游戏程序或 macOS .app 应用"
              />
              <button
                type="button"
                disabled={busy}
                onClick={() =>
                  void act(async () => {
                    const result = await api.pickPath('executable');
                    if (result) setExecutable(result);
                  })
                }
              >
                选择
              </button>
            </div>
            <p className="inline-note">
              macOS 可直接选择 .app
              应用。若存在多个入口，请进入包内选择实际游戏程序。
            </p>
            {error && (
              <p role="alert" className="modal-error">
                {error}
              </p>
            )}
            <div className="modal-actions">
              <button
                type="button"
                className="secondary"
                disabled={busy}
                onClick={() => setImporting(false)}
              >
                取消
              </button>
              <button
                className="primary"
                disabled={busy || !executable || !name.trim()}
              >
                {busy && <LoaderCircle size={15} className="spin" />}导入安装
              </button>
            </div>
          </form>
        </Modal>
      )}
      {detailedInstallation && (
        <InstallationDetails
          installation={detailedInstallation}
          data={data}
          onClose={() => setInstallationDetailsId(null)}
          onInstances={() => {
            setInstanceFilter(detailedInstallation.id);
            setGameView('instances');
            setInstallationDetailsId(null);
          }}
        />
      )}
      {launching && (
        <Modal
          title={plan ? '确认启动环境' : `启动 ${launching.name}`}
          onClose={closeLaunch}
        >
          {!plan ? (
            <form
              onSubmit={(event) => {
                event.preventDefault();
                void act(async () =>
                  setPlan(
                    await api.launchPlan(
                      launching.id,
                      parseArguments(argsText),
                    ),
                  ),
                );
              }}
            >
              <p className="muted">
                参数将作为独立参数传递，不通过 shell 执行。默认不追加任何参数。
              </p>
              <label className="field-label" htmlFor="launch-args">
                启动参数（JSON 字符串数组）
              </label>
              <textarea
                id="launch-args"
                value={argsText}
                onChange={(event) => setArgsText(event.target.value)}
                spellCheck={false}
              />
              {error && (
                <p role="alert" className="modal-error">
                  {error}
                </p>
              )}
              <div className="modal-actions">
                <button
                  type="button"
                  className="secondary"
                  onClick={closeLaunch}
                  disabled={busy}
                >
                  取消
                </button>
                <button className="primary" disabled={busy}>
                  {busy && <LoaderCircle size={15} className="spin" />}
                  生成启动预览
                  <ArrowRight size={15} />
                </button>
              </div>
            </form>
          ) : (
            <>
              <dl className="launch-details">
                <dt>可执行文件</dt>
                <dd>{plan.executable}</dd>
                <dt>工作目录</dt>
                <dd>{plan.working_dir}</dd>
                <dt>启动参数</dt>
                <dd>{JSON.stringify(plan.args)}</dd>
                <dt>工作区存档目录（未绑定）</dt>
                <dd>{plan.save_dir}</dd>
                <dt>程序 SHA-256</dt>
                <dd className="hash">{plan.executable_sha256}</dd>
              </dl>
              <div className="plan-warnings">
                <TriangleAlert size={18} />
                <ul>
                  {plan.warnings.map((w) => (
                    <li key={w}>{w}</li>
                  ))}
                </ul>
              </div>
              {error && (
                <p role="alert" className="modal-error">
                  {error}
                </p>
              )}
              <div className="modal-actions">
                <button
                  className="secondary"
                  disabled={busy}
                  onClick={closeLaunch}
                >
                  取消
                </button>
                <button
                  className="primary"
                  disabled={busy}
                  onClick={() =>
                    void act(async () => {
                      await api.launch(plan.id);
                      setLaunching(null);
                      setPlan(null);
                      setNotice('启动请求已执行，进程结果将在本地记录。');
                    })
                  }
                >
                  {busy ? (
                    <LoaderCircle size={16} className="spin" />
                  ) : (
                    <Gamepad2 size={16} />
                  )}
                  确认启动
                </button>
              </div>
            </>
          )}
        </Modal>
      )}
      {acknowledging && (
        <Modal
          title="确认上次游戏已关闭"
          onClose={() => {
            if (!busy) setAcknowledging(null);
          }}
        >
          <p>
            请先在系统中检查游戏及其子进程均已关闭。此确认只解除启动阻止，不会将存档标记为稳定，也不会创建检查点。
          </p>
          {error && (
            <p role="alert" className="modal-error">
              {error}
            </p>
          )}
          <div className="modal-actions">
            <button
              className="secondary"
              disabled={busy}
              onClick={() => setAcknowledging(null)}
            >
              返回检查
            </button>
            <button
              className="primary"
              disabled={busy}
              onClick={() =>
                void act(async () => {
                  await api.acknowledge(acknowledging);
                  setAcknowledging(null);
                })
              }
            >
              我已检查，游戏已关闭
            </button>
          </div>
        </Modal>
      )}
    </div>
  );
}
function EmptyLibrary({
  onImport,
  disabled,
  needsWorkspace,
  onSetup,
}: {
  onImport: () => void;
  disabled: boolean;
  needsWorkspace?: boolean;
  onSetup?: () => void;
}) {
  return (
    <section className="panel empty-library">
      <div className="empty-game-icon">
        <Gamepad2 size={36} />
      </div>
      <span className="eyebrow">YOUR NEXT EXPEDITION STARTS HERE</span>
      <h2>给下一次出发，留一个位置。</h2>
      <p>导入已有的 Cataclysm 安装，集中查看版本、路径与运行状态。</p>
      {needsWorkspace ? (
        <button className="primary" onClick={onSetup}>
          <FolderOpen size={16} />
          先配置工作区
        </button>
      ) : (
        <button className="primary" onClick={onImport} disabled={disabled}>
          <Plus size={16} />
          导入第一个游戏
        </button>
      )}
      <small>支持本地安装 · 不强制下载 · 不改写游戏文件</small>
    </section>
  );
}
