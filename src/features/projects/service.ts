import { command } from '../../lib/backend';
export const projects = {
  create: (name: string, profileId?: string, semanticClasses?: string[]) =>
    command('create_project', {
      name,
      ...(profileId ? { profileId } : {}),
      ...(semanticClasses ? { semanticClasses } : {}),
    }),
  open: () => command('open_project', undefined),
  close: () => command('close_project', undefined),
};
