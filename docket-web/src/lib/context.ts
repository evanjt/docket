import { getContext, setContext } from 'svelte';
import type { Board } from './flow';
import type { ProjectRow } from './types';

export interface ProjectContext {
  readonly slug: string;
  readonly row: ProjectRow | undefined;
  readonly board: Board | undefined;
  /** The current page with an item open beside it. */
  item: (id: string) => string;
}

const KEY = Symbol('project');

export function provide(ctx: ProjectContext) {
  setContext(KEY, ctx);
}

export function project(): ProjectContext {
  return getContext<ProjectContext>(KEY);
}
