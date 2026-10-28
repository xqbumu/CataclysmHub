import { useState } from 'react';
import { Pencil } from 'lucide-react';
import Modal from './Modal';
import type { BackupDetailsInput, BackupRecord, HubApi } from './api';
function draftFor(backup: BackupRecord): BackupDetailsInput {
  return {
    backup_id: backup.id,
    display_name: backup.display_name,
    notes: backup.notes,
    retention_locked: backup.retention_locked,
    expected_revision: backup.metadata_revision,
  };
}
export default function BackupDetailsEditor({
  api,
  backup,
  busy,
  error,
  run,
}: {
  api: HubApi;
  backup: BackupRecord;
  busy: boolean;
  error: string;
  run: (action: () => Promise<void>) => Promise<void>;
}) {
  const [draft, setDraft] = useState<BackupDetailsInput | null>(null);
  const nameLength = Array.from(draft?.display_name.trim() ?? '').length;
  const notesLength = Array.from(draft?.notes ?? '').length;
  const invalid = nameLength > 128 || notesLength > 4000;
  return (
    <>
      <button
        className="secondary"
        disabled={busy || !api.desktop}
        onClick={() => setDraft(draftFor(backup))}
      >
        <Pencil size={14} />
        编辑名称与保留锁
      </button>
      {draft && (
        <Modal
          title="编辑备份信息"
          onClose={() => {
            if (!busy) setDraft(null);
          }}
        >
          <p className="muted">
            仅更新本机名称、备注和保留标记。归档内容、UUID
            目录名及校验记录保持原样。
          </p>
          <div className="backup-detail-field">
            <label htmlFor={`backup-name-${backup.id}`}>备份名称</label>
            <input
              id={`backup-name-${backup.id}`}
              aria-describedby={`backup-name-help-${backup.id}`}
              value={draft.display_name}
              onChange={(event) =>
                setDraft({ ...draft, display_name: event.target.value })
              }
              placeholder="例如：升级前 · 长期存档"
            />
            <small id={`backup-name-help-${backup.id}`}>
              {nameLength} / 128 字符；留空恢复为创建时间展示
            </small>
          </div>
          <div className="backup-detail-field">
            <label htmlFor={`backup-notes-${backup.id}`}>备注</label>
            <textarea
              id={`backup-notes-${backup.id}`}
              aria-describedby={`backup-notes-help-${backup.id}`}
              rows={4}
              value={draft.notes}
              onChange={(event) =>
                setDraft({ ...draft, notes: event.target.value })
              }
            />
            <small id={`backup-notes-help-${backup.id}`}>
              {notesLength} / 4000 字符
            </small>
          </div>
          <label className="backup-ack">
            <input
              type="checkbox"
              checked={draft.retention_locked}
              onChange={(event) =>
                setDraft({ ...draft, retention_locked: event.target.checked })
              }
            />
            <span>保留锁定</span>
          </label>
          <p className="muted">
            当前尚未开放清理和删除。命名与保留锁会用于管理器保留规则，不限制操作系统的文件删除；这些标注仅保存在本机元数据中。
          </p>
          {backup.metadata_revision !== draft.expected_revision && (
            <div className="alert">
              备份信息已更新，保存旧草稿会被拒绝。可取消后重新编辑，或载入最新信息替换草稿。
              <button
                className="secondary"
                disabled={busy}
                onClick={() =>
                  void run(async () => {
                    const latest = (await api.dashboard()).backups.find(
                      (item) => item.id === backup.id,
                    );
                    if (!latest)
                      throw new Error('备份记录不存在，请刷新列表。');
                    setDraft(draftFor(latest));
                  })
                }
              >
                载入最新信息并替换草稿
              </button>
            </div>
          )}
          {invalid && (
            <p role="alert" className="modal-error">
              名称或备注超过字符上限，请缩短后保存。
            </p>
          )}
          {error && (
            <p role="alert" className="modal-error">
              {error}
            </p>
          )}
          <div className="modal-actions">
            <button
              className="secondary"
              disabled={busy}
              onClick={() => setDraft(null)}
            >
              取消
            </button>
            <button
              className="primary"
              disabled={busy || invalid}
              onClick={() =>
                void run(async () => {
                  await api.updateBackupDetails(draft);
                  setDraft(null);
                })
              }
            >
              保存备份信息
            </button>
          </div>
        </Modal>
      )}
    </>
  );
}
