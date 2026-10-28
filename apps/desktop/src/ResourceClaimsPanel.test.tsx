import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { expect, it, vi } from 'vitest';
import ResourceClaimsPanel from './ResourceClaimsPanel';
import type { ResourceClaim } from './bindings/ResourceClaim';

const unknown: ResourceClaim = {
  owner_kind: 'session',
  owner_id: 'old-session',
  operation: 'game.launch',
  state: 'interrupted_unknown',
  scope: 'global',
  access: 'exclusive',
  path: null,
  reason: 'unknown_game_writes',
};

it('allows an unknown owner outside recent history to enter the existing confirmation flow', async () => {
  const acknowledge = vi.fn();
  render(
    <ResourceClaimsPanel
      claims={[
        unknown,
        {
          ...unknown,
          scope: 'directory',
          access: 'write',
          path: '/isolated/save',
          reason: 'declared_directory',
        },
      ]}
      onAcknowledge={acknowledge}
    />,
  );
  expect(screen.getByText('/isolated/save')).toBeInTheDocument();
  expect(screen.getByText(/目录列表仅表示已知保护范围/)).toBeInTheDocument();
  await userEvent.click(
    screen.getByRole('button', { name: '检查并确认游戏关闭' }),
  );
  expect(acknowledge).toHaveBeenCalledWith('old-session');
});

it('does not treat an empty claim list as verified binding or accepted concurrency', () => {
  render(<ResourceClaimsPanel claims={[]} onAcknowledge={vi.fn()} />);
  expect(screen.getByText(/绑定与存档兼容性仍需单独验证/)).toBeInTheDocument();
  expect(screen.getByText(/正式多版本并发支持尚未验收/)).toBeInTheDocument();
  expect(screen.queryByRole('button')).not.toBeInTheDocument();
});

it('counts directory-only candidates once each and displays every protected scope', () => {
  const claims: ResourceClaim[] = ['a', 'b'].flatMap((id) =>
    ['install', 'instance'].map((role) => ({
      ...unknown,
      owner_id: id,
      state: 'running',
      scope: 'directory',
      access: 'write',
      path: `/${id}/${role}`,
      reason: 'declared_directory',
    })),
  );
  render(<ResourceClaimsPanel claims={claims} onAcknowledge={vi.fn()} />);
  expect(
    screen.getByText('共有 2 个会话或任务正在保护资源。'),
  ).toBeInTheDocument();
  for (const claim of claims)
    expect(screen.getByText(claim.path!)).toBeInTheDocument();
  expect(screen.queryByText(/当前没有活动资源占用/)).not.toBeInTheDocument();
  expect(screen.getAllByText(/会话按声明目录保护资源/)).toHaveLength(2);
  expect(screen.queryByRole('button')).not.toBeInTheDocument();
});
