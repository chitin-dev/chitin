//! Stable native-layout failure codes, independent of formatted error messages.

/// The native stage that rejected a complete coordinate proposal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeStage {
  /// Detached topology construction and canonical remapping.
  Topology,
  /// Temporary ring-edge removal and closure restoration.
  RingOpening,
  /// Immutable template coordinate normalization.
  Template,
  /// Macrocycle contour matching and attachment counts.
  Macrocycle,
  /// Initial fragment-frame composition.
  Assembly,
  /// Discrete candidate reconstruction.
  CandidateAssembly,
  /// Scoped discrete coordinate actions.
  Move,
  /// Discrete candidate scoring and selection.
  Search,
  /// Force integration and its geometry evaluation.
  Simulation,
  /// Interaction evaluation shared by discrete search and force integration.
  Evaluation,
  /// Required local and final stereochemical checks.
  Stereo,
  /// Final coordinate orientation.
  Orientation,
}

impl NativeStage {
  /// Returns a stable machine-readable stage identifier.
  pub const fn code(self) -> &'static str {
    match self {
      Self::Topology => "topology",
      Self::RingOpening => "ring_opening",
      Self::Template => "template",
      Self::Macrocycle => "macrocycle",
      Self::Assembly => "assembly",
      Self::CandidateAssembly => "candidate_assembly",
      Self::Move => "move",
      Self::Search => "search",
      Self::Simulation => "simulation",
      Self::Evaluation => "evaluation",
      Self::Stereo => "stereo",
      Self::Orientation => "orientation",
    }
  }
}

/// A precise rejection condition, not a diagnosis that source chemistry is invalid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeFailureKind {
  /// A required fragment attachment direction has zero length.
  DegenerateAttachment,
  /// A reflection axis has zero or near-zero length.
  DegenerateReflectionAxis,
  /// Coordinates contain NaN or infinity.
  NonFiniteCoordinates,
  /// Interaction energy is NaN or infinity.
  NonFiniteEnergy,
  /// Discrete candidate score is NaN or infinity.
  NonFiniteScore,
  /// Template normalization has no positive drawing scale.
  InvalidTemplateScale,
  /// Fixed masks and coordinate arrays have different dimensions.
  InvalidSimulationDimensions,
  /// A canonical remapping consumes the same entity more than once.
  InvalidTopologyRemap,
  /// A requested ring cut is absent or is not cyclic.
  InvalidRingCut,
  /// The restored topology has no closure edge.
  MissingClosureEdge,
  /// A required coordinate is absent from a local frame.
  MissingFrameAtom,
  /// A required source stereo bond is absent from detached topology.
  MissingStereoBond,
  /// A scoped action's pivot is absent from its local frame.
  MissingMovePivot,
  /// A scoped action's member is absent from its local frame.
  MissingMoveMember,
  /// Every missing scope member is the incoming parent ghost excluded on rebuild.
  ExcludedParentGhost,
  /// The submitted move state exceeds its finite domain.
  InvalidMoveState,
  /// Move and state-vector dimensions differ.
  InvalidStateVector,
  /// Search has an empty domain or zero candidate limit.
  InvalidSearchDomain,
  /// Search produced no completely scored candidate.
  MissingSearchCandidate,
  /// A contour rule references an absent cyclic slot.
  InvalidContourRules,
  /// A reachable attachment count exceeds its integer representation.
  AttachmentCountOverflow,
}

impl NativeFailureKind {
  /// Returns a stable machine-readable rejection identifier.
  pub const fn code(self) -> &'static str {
    match self {
      Self::DegenerateAttachment => "degenerate_attachment",
      Self::DegenerateReflectionAxis => "degenerate_reflection_axis",
      Self::NonFiniteCoordinates => "nonfinite_coordinates",
      Self::NonFiniteEnergy => "nonfinite_energy",
      Self::NonFiniteScore => "nonfinite_score",
      Self::InvalidTemplateScale => "invalid_template_scale",
      Self::InvalidSimulationDimensions => "invalid_simulation_dimensions",
      Self::InvalidTopologyRemap => "invalid_topology_remap",
      Self::InvalidRingCut => "invalid_ring_cut",
      Self::MissingClosureEdge => "missing_closure_edge",
      Self::MissingFrameAtom => "missing_frame_atom",
      Self::MissingStereoBond => "missing_stereo_bond",
      Self::MissingMovePivot => "missing_move_pivot",
      Self::MissingMoveMember => "missing_move_member",
      Self::ExcludedParentGhost => "excluded_parent_ghost",
      Self::InvalidMoveState => "invalid_move_state",
      Self::InvalidStateVector => "invalid_state_vector",
      Self::InvalidSearchDomain => "invalid_search_domain",
      Self::MissingSearchCandidate => "missing_search_candidate",
      Self::InvalidContourRules => "invalid_contour_rules",
      Self::AttachmentCountOverflow => "attachment_count_overflow",
    }
  }
}

/// Typed provenance retained through layout and rendering error wrappers.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("native layout failure: {}/{}", .stage.code(), .kind.code())]
pub struct NativeLayoutFailure {
  /// Stage at the original rejection site, not the outer caller.
  pub stage: NativeStage,
  /// Condition that actually triggered the rejection.
  pub kind: NativeFailureKind,
}
