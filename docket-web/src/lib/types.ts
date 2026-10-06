export type Kind = 'work' | 'decision' | 'research' | 'audit' | 'story' | 'idea' | 'package';

export interface ProjectRow {
  slug: string;
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
  /** What the item is: task, bug, question, investigation or plan. */
  type?: string;
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
  complexity?: string | null;
  /** The release it is in; none is the backlog. */
  release?: string | null;
  /** The area it is in, by name. */
  area?: string | null;
  /** The labels it carries: its own, then those of each plan above it. */
  labels?: string[];
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
  parent?: string | null;
  origin: string[];
  children: string[];
  cites: Cite[];
  progress?: Progress | null;
}

/** A verb an item takes now, the request fields it is refused without and the branch it is sent with. */
export interface Offer {
  verb: string;
  needs: string[];
  branch?: string | null;
}

export interface Offers {
  id: string;
  verbs: Offer[];
  priorities: string[];
  levels: string[];
}

/** The forecast of a release as `/metrics` works it out. */
export interface Forecast {
  open: number;
  burn: number;
  converging: boolean;
  p50: string | null;
  p85: string | null;
  target: string | null;
  late: boolean | null;
}

/** One unshipped release as `/metrics?scope=releases` counts it. */
export interface ReleaseCounts {
  name: string;
  open: number;
  ready: number;
  building: number;
  waiting_owner: number;
  blocked: number;
  closed: number;
  held_later: number;
  pace: number;
  forecast: Forecast;
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
  priorities: string[];
  levels: string[];
}

export interface Machine {
  name: string;
  ssh: string;
  slots: number;
  runners: string[];
  note: string | null;
  updated_at: string;
  /** Each runner that reported a usage limit here, with the reset it named. */
  limits?: Record<string, string>;
}

export interface Lead {
  project: string;
  host: string;
  session: string;
  branch: string | null;
  since: string;
  renewed_at: string;
}

export interface LeadState {
  project: string;
  lead: Lead | null;
  lapsed: boolean;
  lapses_at: string | null;
  lapse_minutes: number;
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
  release?: string | null;
  /** The area it is in, by name. */
  area?: string | null;
  title: string;
  word: string;
  /** What a plan, story, package or idea holds, as the server counts it. */
  progress?: Progress;
  /** A plan due for its audit, as the server decides it. */
  due?: boolean;
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

/** `/deps`: the items an item is tied to, by the tie; mentions carry a snippet, same files a shared count. */
export interface Deps {
  holds?: Row[];
  group?: Row[];
  mentions?: (Row & { snip?: string })[];
  same_files?: (Row & { shared?: number })[];
}

/** One area as `/areas` lists it. */
export interface Area {
  id: number;
  name: string;
  description: string | null;
  position: number;
  priority: string | null;
}

export interface FactSet {
  project: string;
  key: string;
  skills: Record<string, string>;
}

/** One integrity problem of `/check`, with the fields its kind carries. */
export interface Problem {
  kind: string;
  id?: string;
  by?: string;
  release?: string;
  later?: string;
  [field: string]: unknown;
}

/** A claim of `/summary`: its flag when nothing has moved on it for longer than the stale limit. */
export interface Claim {
  id: string;
  title: string;
  branch: string | null;
  host: string;
  since: number | null;
  flag: string | null;
}

/** `/summary`: the claims with their stale flags, beside the plans and the check. */
export interface Summary {
  claims: Claim[];
  problems: Problem[];
}
