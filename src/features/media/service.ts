import { command } from '../../lib/backend';
export const media = {
  open: () => command('open_image', undefined),
  restore: () => command('restore_image', undefined),
};
