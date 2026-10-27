//! Semantic label recipes shared by measurement requests and final placement.

use super::{DepictError as Error, DepictOptions, HydrogenMode, hydrogens};
use crate::font::TextRequest;
use chitin_chem::{AtomId, Element, ImplicitHydrogenPolicy, MolecularGraph, perception::analyze_valence};
use std::collections::{BTreeMap, BTreeSet};

/// Chemical label fragments before a font is measured or geometry is assigned.
pub(super) struct Recipe {
  /// Source atom, never a temporary label or glyph index.
  pub id: AtomId,
  /// Element determines the base text and elemental color.
  pub element: Element,
  /// Literal leading superscript, if supplied by the input isotope.
  pub isotope: Option<String>,
  /// Attached H left after any depiction-only stereo H expansion.
  pub hydrogens: u16,
  /// Literal trailing superscript, absent for a neutral atom.
  pub charge: Option<String>,
}

impl Recipe {
  /// Matches positive/negative charge enclosures by magnitude, not glyph shape.
  pub(super) fn charge_variants(&self) -> Option<[String; 2]> {
    self.charge.as_ref().map(|charge| {
      let magnitude = charge.trim_end_matches(['+', '-']);
      [format!("{magnitude}+"), format!("{magnitude}-")]
    })
  }
  /// Returns the H-count subscript, absent for zero or one attached H.
  pub(super) fn hydrogen_count(&self) -> Option<String> {
    (self.hydrogens > 1).then(|| self.hydrogens.to_string())
  }

  /// Enumerates exactly the fragments used by ordinary label placement.
  fn requests(&self, size: f64, requests: &mut BTreeSet<TextRequest>) -> Result<(), Error> {
    requests.insert(TextRequest::new(self.element.symbol(), size)?);
    if self.hydrogens > 0 {
      requests.insert(TextRequest::new("H", size)?);
    }
    for content in [self.isotope.clone(), self.hydrogen_count(), self.charge.clone()]
      .into_iter()
      .flatten()
    {
      requests.insert(TextRequest::new(content, size * 0.7)?);
    }
    Ok(())
  }
}

/// Resolves visibility and H counts once without reading font metrics.
///
/// # Returns
/// Stable atom-keyed recipes. Ordinary and host-measured drawing use the same
/// recipes, so isotope, charge, H-count and hidden-carbon policies cannot drift.
///
/// # Errors
/// Propagates missing facts and unresolved H before any text is requested.
pub(super) fn prepare(
  graph: &MolecularGraph,
  options: &DepictOptions,
  expanded: &BTreeSet<AtomId>,
) -> Result<BTreeMap<AtomId, Recipe>, Error> {
  let report = match options.hydrogens {
    HydrogenMode::KnownOnly => None,
    HydrogenMode::Infer(model) => Some(analyze_valence(graph, model)?),
  };
  let mut recipes = BTreeMap::new();
  for (id, atom) in graph.atoms() {
    let charge = atom.formal_charge().ok_or(Error::UnknownFacts(id))?;
    let declared = atom.declared_hydrogens().ok_or(Error::UnknownFacts(id))?;
    if atom.unpaired_electrons().is_some_and(|count| count > 0) {
      return Err(Error::Radical(id));
    }
    let hidden = atom.element() == Element::Carbon
      && atom.isotope().is_none()
      && charge == 0
      && ((declared == 0
        && atom.unpaired_electrons() == Some(0)
        && atom.implicit_hydrogen_policy() == ImplicitHydrogenPolicy::Infer)
        || graph.tetrahedral(id).is_some())
      && graph.neighbors(id)?.len() != 0;
    if hidden {
      continue;
    }
    let implicit = match atom.implicit_hydrogen_policy() {
      ImplicitHydrogenPolicy::Fixed(count) => count,
      ImplicitHydrogenPolicy::Infer => report
        .as_ref()
        .and_then(|report| report.atom(id))
        .and_then(|row| row.value())
        .map(|value| value.hydrogens.implicit())
        .ok_or(Error::Hydrogens(id))?,
      ImplicitHydrogenPolicy::Unspecified => return Err(Error::Hydrogens(id)),
    };
    if atom.element() == Element::Hydrogen && (declared != 0 || implicit != 0) {
      return Err(Error::Hydrogens(id));
    }
    let sign = if charge > 0 { '+' } else { '-' };
    let charge = (charge != 0).then(|| {
      if charge.unsigned_abs() == 1 {
        sign.to_string()
      } else {
        format!("{}{sign}", charge.unsigned_abs())
      }
    });
    recipes.insert(
      id,
      Recipe {
        id,
        element: atom.element(),
        isotope: atom.isotope().map(|isotope| isotope.mass_number().to_string()),
        hydrogens: u16::from(declared) + u16::from(implicit) - u16::from(expanded.contains(&id)),
        charge,
      },
    );
  }
  Ok(recipes)
}

/// Lists deduplicated exact runs for an external font-measuring host.
///
/// # Parameters
/// - `graph`: immutable chemical input, including supported aromatic facts.
/// - `options`: final drawing sizes and H policy; `font` is not consulted.
///
/// # Returns
/// Deterministically ordered text/size pairs, including scripts and expanded
/// chiral H. A host measures these using its resolved font and returns them via
/// `FontFace::from_measurements`. Then call `depict` with unchanged graph and
/// options plus that font. No placeholder geometry or default-font metrics are
/// generated. This stage needs no layout coordinates or platform font lookup.
/// Automatic stereo-H expansion conservatively requests an H glyph even when
/// a later nondegenerate projection can use a source bond and omit that glyph.
///
/// # Errors
/// Rejects invalid sizes, unsupported chemistry, unknown facts and unresolved H.
/// Geometry-dependent depiction can still fail after measurement succeeds.
///
/// # Examples
/// ```
/// use chitin_chem::io::smiles::read_smiles;
/// use chitin_chemdraw::{depict, font::FontFace, layout};
/// let graph = read_smiles("[13NH2+]CO")?.into_graph();
/// let mut options = depict::DepictOptions::default();
/// let requests = depict::measurement_requests(&graph, &options)?;
/// // A real host measures with its own font engine; this example uses the
/// // bundled provider to demonstrate the transport-independent contract.
/// let entries = requests.into_iter().map(|request| {
///   let metrics = FontFace::NotoSans.measure(request.content(), request.font_size())?;
///   Ok((request, metrics))
/// }).collect::<Result<Vec<_>, chitin_chemdraw::font::FontError>>()?;
/// options.font = FontFace::from_measurements("host-font/regular", entries)?;
/// let coordinates = layout::generate(&graph, &layout::LayoutOptions::default())?;
/// let scene = depict::depict(&graph, &coordinates, &options)?;
/// // The host renders scene text using the same identity and measured settings.
/// assert!(!scene.primitives().is_empty());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn measurement_requests(graph: &MolecularGraph, options: &DepictOptions) -> Result<Vec<TextRequest>, Error> {
  super::validate_options(options)?;
  let resolved = crate::chemical::integer_bonds(graph)?;
  let graph = resolved.as_ref();
  for (id, bond) in graph.bonds() {
    if !matches!(
      bond.order(),
      chitin_chem::BondOrder::Single | chitin_chem::BondOrder::Double | chitin_chem::BondOrder::Triple
    ) {
      return Err(Error::Bond(id));
    }
  }
  let mut requests = BTreeSet::new();
  for recipe in prepare(graph, options, &hydrogens::centers(graph, options))?.values() {
    recipe.requests(options.font_size, &mut requests)?;
    if options.charge_style == super::ChargeStyle::Circled
      && let Some(variants) = recipe.charge_variants()
    {
      for content in variants {
        requests.insert(TextRequest::new(content, options.font_size * 0.7)?);
      }
    }
  }
  if hydrogens::may_expand(graph, options) {
    requests.insert(TextRequest::new("H", options.font_size)?);
  }
  Ok(requests.into_iter().collect())
}
