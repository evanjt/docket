export type Kind = 'work' | 'decision' | 'research' | 'audit' | 'story' | 'concept' | 'idea' | 'package';

export interface KeySpec {
  key: string;
  kind: Kind;
  meaning?: string | null;
  turn?: string | null;
}

export interface ProjectRow {
  slug: string;
  keys: KeySpec[];
  themes: { name: string; note?: string | null }[];
  skills: Record<string, string>;
  updated_at: string;
}

export interface Count {
  slug: string;
  open: number;
  done: number;
  dropped: number;
  last_event: string | null;
}

export interface Row {
  id: string;
  key: string;
  num: number;
  project: string;
  title: string;
  state: string;
  word: string;
  priority: string;
  group?: string | null;
  superseded_by?: string | null;
  turn?: string | null;
  turn_note?: string | null;
  asked_at?: string | null;
  claim_branch?: string | null;
  claim_host?: string | null;
  claim_since?: string | null;
  claim_runner?: string | null;
  claim_job?: string | null;
  claim_on?: string | null;
  wait_on?: string | null;
  wait_ref?: string | null;
  wait_since?: string | null;
  decision?: string | null;
  decided_at?: string | null;
  resolution?: string | null;
  scope?: string | null;
  complexity?: string | null;
  theme?: string | null;
  rank?: number | null;
  tags: string[];
  body: string;
  opened_at: string;
  updated_at: string;
  snip?: string | null;
}

export interface Cite {
  path?: string | null;
  line?: number | null;
  kind: string;
}

export interface Progress {
  done: number;
  total: number;
  live: number;
}

export interface Shown extends Row {
  related: string[];
  opened: string[];
  cites: Cite[];
  progress?: Progress | null;
}

export interface Status {
  project: string;
  host: string;
  total: number;
  by_word: Record<string, number>;
  by_key: Record<string, Record<string, number>>;
}

export interface EventRow {
  seq: number;
  project: string;
  rid?: number | null;
  at: string;
  host: string;
  branch?: string | null;
  kind: string;
  note?: string | null;
}

export interface Derived {
  id: string;
  state: string;
  at: string;
  title: string;
  chose?: string | null;
  basis: string;
}

export interface Facts {
  project: string;
  skills: Record<string, string>;
  last_tick: string | null;
}

export interface Whoami {
  host: string;
  owner: boolean;
}

export interface GraphNode {
  id: string;
  rid?: number;
  key: string;
  kind: Kind;
  state: string;
  theme: string | null;
  title: string;
  word: string;
}

export interface GraphEdge {
  from: string;
  kind: string;
  to: string;
}

export interface Graph {
  project: string;
  nodes: GraphNode[];
  edges: GraphEdge[];
}

export interface Context {
  concepts: string[];
  holds: string[];
  members: string[];
  no_concept: boolean;
  package: string | null;
  priority: string;
  raised_by: string | null;
  standing: string | null;
}

export interface FactSet {
  project: string;
  key: string;
  skills: Record<string, string>;
}
