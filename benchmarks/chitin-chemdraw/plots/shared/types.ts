/** Disk protocol is versioned independently from presentation; unknown metrics remain visible. */
export interface Observation {
  state: "measured" | "not_applicable" | "not_implemented";
  value: number | null;
  eligible: number;
  violations: number | null;
  reason: string | null;
}
export interface Features {
  atoms: number;
  bonds: number;
  maximum_block_rank: number;
  tetrahedral_centers: number;
  double_bond_descriptors: number;
}
export interface CaseRecord {
  case: { row: number; id: string; smiles: string };
  status: string;
  stage: string;
  error: string | null;
  features: Features | null;
  metrics: {
    version: string;
    profile: string;
    measurements: Record<string, Observation>;
    readability_clear: boolean | null;
  } | null;
  timings_ms: Record<string, number>;
}
export interface Manifest {
  schema_version: number;
  executable_sha256?: string;
  renderer_executable?: string;
  input_sha256: string;
  quality_spec_sha256?: string;
  config: { dataset: string; version: string; input: string; max_atoms: number; seed: number; timeout_seconds: number };
}
export interface RunInfo {
  id: string;
  updated: number;
  revision: string;
  dataset: string;
  version: string;
  cases: number;
  evaluated: number;
  protocol: string | null;
}
/** Full records stay server-side; only the fields needed by charts cross the network. */
export interface PlotPoint {
  row: number;
  id: string;
  atoms: number;
  rank: number;
  contacts: number | null;
  length: number | null;
  angle: number | null;
  profile: string;
}
export interface MetricSummary {
  measured_cases: number;
  affected_cases: number | null;
  not_applicable_cases: number;
  not_implemented_cases: number;
}
export interface PlotData {
  run: RunInfo;
  revision: string;
  points: PlotPoint[];
  metrics: Record<string, MetricSummary>;
  outcomes: Record<string, number>;
  completed: number;
  clear: number;
}
export interface RenderResult {
  url: string;
  provenance: "saved" | "snapshot" | "current";
  fingerprint: string;
  warning: string | null;
}
export const GROUP_LABELS = ["0", "1", "2–3", "≥4"];
export const GROUP_COLORS = ["#64748b", "#38a6d8", "#2864bc", "#ce6c32"];
export function rankGroup(rank: number): number {
  return rank === 0 ? 0 : rank === 1 ? 1 : rank <= 3 ? 2 : 3;
}
