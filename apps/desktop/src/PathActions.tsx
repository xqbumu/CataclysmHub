import { useState } from 'react';
import { errorMessage, type HubApi } from './api';
export default function PathActions({
  api,
  path,
}: {
  api: HubApi;
  path: string;
}) {
  const [message, setMessage] = useState('');
  async function run(action: () => Promise<void>, success: string) {
    try {
      await action();
      setMessage(success);
    } catch (e) {
      setMessage(errorMessage(e));
    }
  }
  return (
    <div>
      <button
        className="file-path"
        title="点击复制路径"
        onClick={() =>
          void run(() => navigator.clipboard.writeText(path), '路径已复制')
        }
      >
        {path}
      </button>
      <button
        disabled={!api.desktop}
        onClick={() =>
          void run(() => api.openWorkspacePath(path), '已打开文件浏览器')
        }
      >
        打开目录
      </button>
      {message && <small role="status">{message}</small>}
    </div>
  );
}
