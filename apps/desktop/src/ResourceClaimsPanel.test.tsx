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
  expect(screen.getByText(/多版本并发尚未开放/)).toBeInTheDocument();
  expect(screen.queryByRole('button')).not.toBeInTheDocument();
});
