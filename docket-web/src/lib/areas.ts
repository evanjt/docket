import { isClosed } from './flow';
import type { Area, GraphNode, Kind } from './types';

/** The kinds that hold other items; an area counts the work in them, not them. */
const HOLDERS: Kind[] = ['audit', 'story', 'package', 'idea'];

export interface AreaCount {
  area: Area;
  open: number;
  done: number;
}

/** Each area in position order with the items it holds, open and done; plans and dropped items are not counted. */
export function areaCounts(areas: Area[], nodes: Iterable<GraphNode>): AreaCount[] {
  const list = [...nodes].filter((n) => !HOLDERS.includes(n.kind) && n.word !== 'dropped');
  return [...areas]
    .sort((a, z) => a.position - z.position)
    .map((area) => {
      const mine = list.filter((n) => n.area === area.name);
      return { area, open: mine.filter((n) => !isClosed(n.word)).length, done: mine.filter((n) => n.word === 'done').length };
    });
}

/** The name of the area an item is in, from the graph. */
export function areaOf(nodes: Map<string, GraphNode> | undefined, id: string): string | null {
  return nodes?.get(id)?.area ?? null;
}
