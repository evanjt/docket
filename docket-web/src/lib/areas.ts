import type { GraphNode } from './types';

/** The name of the area an item is in, from the graph. */
export function areaOf(nodes: Map<string, GraphNode> | undefined, id: string): string | null {
  return nodes?.get(id)?.area ?? null;
}
