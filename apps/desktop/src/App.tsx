import Modal from './Modal';
import BackupsPage from './BackupsPage';
import WorldsPage from './WorldsPage';
import { useCallback, useEffect, useState, type ReactNode } from 'react';
import {
  Activity,
  Archive,
  ArrowRight,
  ArrowUpRight,
  ChevronRight,
  CircleDot,
  Clock3,
  FileText,
  FolderOpen,
  Gamepad2,
  GitBranch,
  Globe2,
  HardDrive,
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
  'overview' | 'games' | 'worlds' | 'backups' | 'activity' | 'settings';
const pages: Record<Page, string> = {
  overview: '概览',
  games: '游戏库',
  worlds: '世界与角色',
  backups: '独立备份',
  activity: '操作记录',
  settings: '工作区',
};
const pathFields: [keyof WorkspaceInput, string, string][] = [
  ['save_dir', '存档根目录', '选择游戏现有的存档目录，只读发现，不修改原文件'],
  ['backup_dir', '独立备份目录', '选择与存档分离的目录，可使用外置磁盘'],
  ['content_dir', '资源目录', '用于管理 mods、音效、图块集与字体'],
];
const statusNames: Record<string, string> = {
  starting: '正在启动',
  running: '运行中',
  exited_unverified: '进程已退出 · 未验证存档',
  failed: '启动失败',
  interrupted_unknown: '上次运行状态未知',
  resolved_unknown: '已人工确认关闭 · 存档未验证',
  succeeded: '已完成',
  cancelling: '正在取消',
  cancelled: '已取消',
  recovery_required: '操作中断，需重新检查',
};
const actionNames: Record<string, string> = {
  'workspace.initialize': '工作区初始化',
  'installation.import': '导入游戏安装',
  'game.launch': '启动游戏',
  'game.process_result': '记录进程结果',
  'process.reconcile': '重启后状态检查',
  'process.user_acknowledged': '人工确认游戏关闭',
  'world.scan': '只读扫描存档目录',
  'backup.create': '创建独立完整备份',
  'backup.verify': '重新校验备份',
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
  const [page, setPage] = useState<Page>('overview');
  const [data, setData] = useState(emptyDashboard);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [paths, setPaths] = useState<WorkspaceInput>({
    save_dir: '',
    backup_dir: '',
    content_dir: '',
  });
  const [importing, setImporting] = useState(false);
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
  const workspace = data.workspace;
  const running = data.sessions.filter(
    (s) => s.state === 'running' || s.state === 'starting',
  );
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
    <section className="panel setup-panel">
      <div className="section-heading">
        <div>
          <span className="eyebrow">FIRST THINGS FIRST</span>
          <h2>{workspace ? '本地工作区' : '建立你的安全工作区'}</h2>
        </div>
        <span className="step-label">{workspace ? '已配置' : '01 / 02'}</span>
      </div>
      <p className="muted">
        将存档、备份与资源分开管理。此设置不会移动文件，也不会修改游戏配置。
      </p>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void act(async () => {
            await api.initialize(paths);
            setNotice('工作区已保存，可以导入你的第一个游戏安装。');
          });
        }}
      >
        {pathFields.map(([key, label, help]) => (
          <div className="path-field" key={key}>
            <label htmlFor={key}>{label}</label>
            <div className="path-input">
              <FolderOpen size={17} />
              <input
                id={key}
                value={workspace?.[key] ?? paths[key]}
                readOnly
                placeholder="尚未选择目录"
              />
              <button
                type="button"
                disabled={busy || !!workspace || !api.desktop}
                onClick={() =>
                  void act(async () => {
                    const result = await api.pickPath('directory');
                    if (result)
                      setPaths((current) => ({ ...current, [key]: result }));
                  })
                }
              >
                选择
              </button>
            </div>
            <small>{help}</small>
          </div>
        ))}
        {!workspace && (
          <div className="form-footer">
            <span>
              <ShieldCheck size={14} /> 数据保留在你的设备上
            </span>
            <button
              className="primary"
              disabled={
                busy || !api.desktop || Object.values(paths).some((p) => !p)
              }
            >
              {busy ? (
                <LoaderCircle className="spin" size={16} />
              ) : (
                <ArrowRight size={16} />
              )}{' '}
              保存工作区
            </button>
          </div>
        )}
        {workspace && (
          <p className="inline-note">
            路径已保存。当前开发版本暂不提供工作区路径迁移，以避免意外改变已有绑定。
          </p>
        )}
      </form>
    </section>
  );

  const gameCards = (
    <div className="game-grid">
      {data.installations.map((game) => {
        const active = data.sessions.find((s) => s.installation_id === game.id);
        const blocked =
          data.sessions.some((s) =>
            ['running', 'starting', 'interrupted_unknown'].includes(s.state),
          ) ||
          data.scan_tasks.some((task) =>
            ['running', 'cancelling'].includes(task.state),
          ) ||
          data.backup_tasks.some((task) =>
            ['running', 'cancelling'].includes(task.state),
          );
        return (
          <article className="panel game-card" key={game.id}>
            <div className="game-art">
              <Radiation size={44} />
              <span>CATACLYSM</span>
              <small>LOCAL INSTALLATION</small>
              <div className="art-grid" />
            </div>
            <div className="game-details">
              <div className="row-between">
                <span className="eyebrow">
                  {game.platform} / {game.arch}
                </span>
                <span className="pill neutral">未验证</span>
              </div>
              <h3>{game.name}</h3>
              <p className="muted">
                版本 {game.version ?? 'unknown'} · 分支{' '}
                {game.branch ?? 'unknown'}
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
                  {statusNames[active.state] ?? active.state}
                </div>
              )}
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
            概览
          </NavItem>
          <NavItem
            icon={Gamepad2}
            selected={page === 'games'}
            onClick={() => setPage('games')}
          >
            游戏库<span className="nav-count">{data.installations.length}</span>
          </NavItem>
          <NavItem
            icon={Globe2}
            selected={page === 'worlds'}
            onClick={() => setPage('worlds')}
          >
            世界与角色
          </NavItem>
          <NavItem icon={GitBranch} soon>
            存档历史
          </NavItem>
          <NavItem
            icon={Archive}
            selected={page === 'backups'}
            onClick={() => setPage('backups')}
          >
            独立备份
          </NavItem>
          <span className="nav-label second">管理</span>
          <NavItem icon={Puzzle} soon>
            内容资源
          </NavItem>
          <NavItem
            icon={FileText}
            selected={page === 'activity'}
            onClick={() => setPage('activity')}
          >
            操作记录
          </NavItem>
          <NavItem
            icon={SlidersHorizontal}
            selected={page === 'settings'}
            onClick={() => setPage('settings')}
          >
            工作区设置
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
              <span className="eyebrow">SURVIVE. ORGANIZE. PRESERVE.</span>
              <h1>
                {page === 'overview' ? '每一次生存，都值得保留。' : pages[page]}
              </h1>
              <p>
                {page === 'overview'
                  ? '管理你的游戏与存档，让下一次出发更从容。'
                  : page === 'games'
                    ? '并存管理本地安装，启动前确认环境与参数。'
                    : page === 'worlds'
                      ? '只读枚举世界候选、共享文件与无法确认的边界。'
                      : page === 'backups'
                        ? '完整归档与独立校验，明确记录采集一致性和文件完整性。'
                        : page === 'activity'
                          ? '查看本地操作和进程状态，追溯每一次变更。'
                          : '分离数据目录，为后续的世界保护建立基础。'}
              </p>
            </div>
            {(page === 'overview' || page === 'games') && (
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
              {page === 'overview' && (
                <>
                  <div className="stats-grid">
                    <Stat
                      icon={Gamepad2}
                      label="本地游戏"
                      value={String(data.installations.length)}
                      detail={
                        running.length
                          ? `${running.length} 个直接进程运行中`
                          : '尚无运行中的已监测进程'
                      }
                    />
                    <Stat
                      icon={Globe2}
                      label="世界存档"
                      value="—"
                      detail="世界候选可在“世界与角色”扫描"
                    />
                    <Stat
                      icon={GitBranch}
                      label="存档检查点"
                      value="—"
                      detail="自动检查点尚未启用"
                    />
                    <Stat
                      icon={Archive}
                      label="完整备份"
                      value={String(data.backups.length)}
                      detail={
                        data.backups.length
                          ? `${data.backups.filter((backup) => backup.verification === 'verified').length} 份最近校验通过 · 尽力快照`
                          : '尚无已验证的独立备份'
                      }
                      warning={
                        data.backups.length === 0 ||
                        data.backups.some(
                          (backup) => backup.verification !== 'verified',
                        )
                      }
                    />
                  </div>
                  {!workspace ? (
                    <div className="overview-grid">
                      {setup}
                      <aside className="guide">
                        <div className="guide-icon">
                          <ShieldCheck size={30} />
                        </div>
                        <span className="eyebrow">
                          BUILT AROUND YOUR WORLDS
                        </span>
                        <h2>
                          先保护世界，
                          <br />
                          再探索未知。
                        </h2>
                        <p>
                          Git
                          历史与独立备份将分别保护你的存档。文件校验、采集稳定性与游戏兼容性会分别显示。
                        </p>
                        <ol>
                          <li className="current">
                            <span>01</span>
                            <div>
                              配置工作区<small>分离存档、备份与资源目录</small>
                            </div>
                          </li>
                          <li>
                            <span>02</span>
                            <div>
                              导入现有游戏
                              <small>选择可执行文件，预览后启动</small>
                            </div>
                          </li>
                          <li>
                            <span>03</span>
                            <div>
                              建立世界保护<small>将在后续开发阶段开放</small>
                            </div>
                          </li>
                        </ol>
                        <div className="guide-bottom">
                          <HardDrive size={15} />
                          不上传存档 · 不修改游戏原文件
                        </div>
                      </aside>
                    </div>
                  ) : (
                    <>
                      <div className="section-heading library-heading">
                        <div>
                          <span className="eyebrow">READY WHEN YOU ARE</span>
                          <h2>你的游戏库</h2>
                        </div>
                        <button
                          className="text-button"
                          onClick={() => setPage('games')}
                        >
                          查看全部
                          <ArrowUpRight size={16} />
                        </button>
                      </div>
                      {data.installations.length ? (
                        gameCards
                      ) : (
                        <EmptyLibrary
                          onImport={openImport}
                          disabled={busy || !api.desktop}
                        />
                      )}
                    </>
                  )}
                  <div className="bottom-grid">
                    <section className="panel recent">
                      <div className="section-heading">
                        <h2>最近动态</h2>
                        <button
                          className="text-button"
                          onClick={() => setPage('activity')}
                        >
                          全部记录
                          <ArrowUpRight size={15} />
                        </button>
                      </div>
                      {data.operations.length ? (
                        data.operations.slice(0, 3).map((op) => (
                          <div className="activity-item" key={op.id}>
                            <span className="activity-icon">
                              <Activity size={15} />
                            </span>
                            <div>
                              <strong>
                                {actionNames[op.action] ?? op.action}
                              </strong>
                              <small>
                                {statusNames[op.status] ?? op.status}
                              </small>
                            </div>
                            <time>{time(op.created_at)}</time>
                          </div>
                        ))
                      ) : (
                        <div className="empty-inline">
                          <Clock3 size={20} />
                          <span>
                            还没有操作记录
                            <small>
                              完成工作区配置后，操作将在这里留下记录。
                            </small>
                          </span>
                        </div>
                      )}
                    </section>
                    <section className="safety-card">
                      <div>
                        <span className="eyebrow">BACKUP HEALTH</span>
                        <span className="pill warning">
                          <TriangleAlert size={12} />
                          {data.backups.length
                            ? '已建立尽力快照'
                            : '尚未建立保护'}
                        </span>
                      </div>
                      <h3>别让唯一的存档，成为唯一的机会。</h3>
                      <p>
                        可在“独立备份”创建尽力快照并校验文件。恢复功能尚未开放，请继续保留现有备份方式。
                      </p>
                    </section>
                  </div>
                </>
              )}
              {page === 'worlds' && (
                <WorldsPage
                  api={api}
                  data={data}
                  busy={busy}
                  run={act}
                  onConfigure={() => setPage('settings')}
                />
              )}
              {page === 'backups' && (
                <BackupsPage
                  api={api}
                  data={data}
                  busy={busy}
                  error={error}
                  run={act}
                  onConfigure={() => setPage('settings')}
                />
              )}
              {page === 'settings' && setup}
              {page === 'games' &&
                (data.installations.length ? (
                  gameCards
                ) : (
                  <EmptyLibrary
                    disabled={!workspace || busy || !api.desktop}
                    onImport={openImport}
                    needsWorkspace={!workspace}
                    onSetup={() => setPage('settings')}
                  />
                ))}
              {page === 'activity' && (
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
                              <td className="mono">{op.task_id.slice(0, 8)}</td>
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
                await api.importInstallation(executable, name);
                setImporting(false);
                setPage('games');
                setNotice('安装已导入。版本暂为 unknown，启动前请核对预览。');
              });
            }}
          >
            <p className="muted">
              选择你信任的游戏可执行文件。这里只建立本地索引，不复制或重新下载游戏。
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
                placeholder="选择可执行文件，不是游戏目录"
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
              macOS 请进入游戏应用包的 Contents/MacOS
              选择实际程序；当前暂不自动识别 .app 启动器。
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
function Stat({
  icon: Icon,
  label,
  value,
  detail,
  warning,
}: {
  icon: LucideIcon;
  label: string;
  value: string;
  detail: string;
  warning?: boolean;
}) {
  return (
    <section className="stat">
      <div className="row-between">
        <span>{label}</span>
        <Icon size={17} />
      </div>
      <strong>{value}</strong>
      <small className={warning ? 'amber-text' : ''}>
        {warning && <span className="status-dot amber" />}
        {detail}
      </small>
    </section>
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
