//! Generates the checked-in typed mmCIF schema from a local PDBx dictionary.

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

use chitin_bio::structure::cif::{CifCategory, CifDocument, CifParser, CifSaveFrame};

const DEFAULT_DICTIONARY: &str = "crates/chitin-bio/src/structure/mmcif/mmcif_pdbx_v50.dic";
const DEFAULT_SELECTION: &str = "crates/chitin-bio/src/structure/mmcif/schema_categories.txt";
const DEFAULT_OUTPUT: &str = "crates/chitin-bio/src/structure/mmcif/schema.rs";

fn main() -> Result<(), GeneratorError> {
  let arguments = Arguments::from_env()?;
  let dictionary_source = fs::read_to_string(&arguments.dictionary).map_err(|source| GeneratorError::Read {
    path: arguments.dictionary.clone(),
    source,
  })?;
  let selection_source = fs::read_to_string(&arguments.selection).map_err(|source| GeneratorError::Read {
    path: arguments.selection.clone(),
    source,
  })?;
  let document = CifParser::parse(&dictionary_source).map_err(GeneratorError::Dictionary)?;
  let selected = parse_selection(&selection_source);
  let schema = extract_schema(&document, &selected)?;
  let rendered = render_schema(&schema)?;
  fs::write(&arguments.output, rendered).map_err(|source| GeneratorError::Write {
    path: arguments.output,
    source,
  })?;
  Ok(())
}

/// Input, selection, and output paths for one generation run.
#[derive(Debug)]
struct Arguments {
  dictionary: PathBuf,
  selection: PathBuf,
  output: PathBuf,
}

impl Arguments {
  /// Reads zero or three positional paths from the process arguments.
  fn from_env() -> Result<Self, GeneratorError> {
    let paths = env::args_os().skip(1).map(PathBuf::from).collect::<Vec<_>>();
    match paths.as_slice() {
      [] => Ok(Self {
        dictionary: DEFAULT_DICTIONARY.into(),
        selection: DEFAULT_SELECTION.into(),
        output: DEFAULT_OUTPUT.into(),
      }),
      [dictionary, selection, output] => Ok(Self {
        dictionary: dictionary.clone(),
        selection: selection.clone(),
        output: output.clone(),
      }),
      _ => Err(GeneratorError::Arguments),
    }
  }
}

/// A dictionary-derived schema restricted to selected categories.
#[derive(Debug)]
struct Schema {
  version: String,
  categories: BTreeMap<String, CategorySchema>,
}

/// One category and its dictionary item definitions.
#[derive(Debug, Default)]
struct CategorySchema {
  items: BTreeMap<String, ItemSchema>,
}

/// One dictionary item required to generate a typed row getter.
#[derive(Debug, Clone)]
struct ItemSchema {
  tag: String,
  type_code: String,
}

/// One raw dictionary item before linked types are resolved.
#[derive(Debug, Clone)]
struct DictionaryItem {
  tag: String,
  type_code: Option<String>,
  parent: Option<String>,
}

/// Failures produced while reading, interpreting, or writing a schema.
#[derive(Debug, thiserror::Error)]
enum GeneratorError {
  /// The command accepted neither the default invocation nor three paths.
  #[error("usage: chitin-mmcif-schema [DICTIONARY SELECTION OUTPUT]")]
  Arguments,
  /// A generator input could not be read.
  #[error("failed to read {path}: {source}")]
  Read { path: PathBuf, source: std::io::Error },
  /// The dictionary is not valid generic CIF.
  #[error("failed to parse the mmCIF dictionary: {0}")]
  Dictionary(chitin_bio::structure::cif::CifParseError),
  /// A selected category was not present in the dictionary.
  #[error("dictionary has no item definitions for selected category `{0}`")]
  MissingCategory(String),
  /// Neither an item nor its linked parents declare a primitive type.
  #[error("dictionary item `{0}` has no resolvable type")]
  MissingItemType(String),
  /// Two dictionary tags normalize to the same Rust method.
  #[error("schema identifier collision in `{category}`: `{first}` and `{second}` both become `{identifier}`")]
  IdentifierCollision {
    category: String,
    first: String,
    second: String,
    identifier: String,
  },
  /// Rendering into an in-memory string unexpectedly failed.
  #[error("failed to render generated Rust source")]
  Render(#[from] std::fmt::Error),
  /// The generated Rust source could not be written.
  #[error("failed to write {path}: {source}")]
  Write { path: PathBuf, source: std::io::Error },
}

/// Parses newline-delimited category names, ignoring comments and blank lines.
fn parse_selection(source: &str) -> BTreeSet<String> {
  source
    .lines()
    .map(str::trim)
    .filter(|line| !line.is_empty() && !line.starts_with('#'))
    .map(str::to_owned)
    .collect()
}

/// Extracts selected item definitions from dictionary save frames.
///
/// # Parameters
///
/// * `document` is the generic CIF representation of the dictionary.
/// * `selected` lists categories that should enter the compiled schema.
///
/// # Returns
///
/// A deterministic schema containing every item from the selected categories,
/// or an error when a requested category is absent.
fn extract_schema(document: &CifDocument, selected: &BTreeSet<String>) -> Result<Schema, GeneratorError> {
  let version = document
    .blocks
    .iter()
    .flat_map(|block| values_for_tag(&block.categories, "_dictionary.version"))
    .next()
    .unwrap_or("unknown")
    .to_owned();
  let mut categories = selected
    .iter()
    .map(|name| (name.clone(), CategorySchema::default()))
    .collect::<BTreeMap<_, _>>();
  let mut dictionary_items = BTreeMap::new();
  for frame in document.blocks.iter().flat_map(|block| &block.save_frames) {
    collect_frame_items(frame, &mut dictionary_items);
  }
  for (child, parent) in linked_parents(document) {
    if let Some(item) = dictionary_items.get_mut(&child) {
      item.parent.get_or_insert(parent);
    }
  }
  for item in dictionary_items.values() {
    let Some(category_name) = category_name(&item.tag) else {
      continue;
    };
    let Some(category) = categories.get_mut(category_name) else {
      continue;
    };
    category.items.insert(
      item.tag.clone(),
      ItemSchema {
        tag: item.tag.clone(),
        type_code: resolve_type(item, &dictionary_items)?,
      },
    );
  }
  for (name, category) in &categories {
    if category.items.is_empty() {
      return Err(GeneratorError::MissingCategory(name.clone()));
    }
  }
  Ok(Schema { version, categories })
}

/// Collects dictionary-wide child-to-parent item relationships.
fn linked_parents(document: &CifDocument) -> BTreeMap<String, String> {
  let mut parents = BTreeMap::new();
  for block in &document.blocks {
    collect_linked_parents(&block.categories, &mut parents);
    for frame in &block.save_frames {
      collect_linked_parents(&frame.categories, &mut parents);
    }
  }
  parents
}

/// Adds child-to-parent pairs from loops in one CIF container.
fn collect_linked_parents(categories: &[CifCategory], parents: &mut BTreeMap<String, String>) {
  for category in categories {
    let CifCategory::Loop { tags, rows } = category else {
      continue;
    };
    let Some(child_index) = tags.iter().position(|tag| tag == "_item_linked.child_name") else {
      continue;
    };
    let Some(parent_index) = tags.iter().position(|tag| tag == "_item_linked.parent_name") else {
      continue;
    };
    for row in rows {
      let Some(child) = row.get(child_index).and_then(|value| value.as_text()) else {
        continue;
      };
      let Some(parent) = row.get(parent_index).and_then(|value| value.as_text()) else {
        continue;
      };
      parents.insert(child.to_owned(), parent.to_owned());
    }
  }
}

/// Adds raw item definitions from one dictionary save frame.
fn collect_frame_items(frame: &CifSaveFrame, items: &mut BTreeMap<String, DictionaryItem>) {
  let type_code = values_for_tag(&frame.categories, "_item_type.code")
    .next()
    .map(str::to_owned);
  let parent = values_for_tag(&frame.categories, "_item_linked.parent_name")
    .next()
    .map(str::to_owned);
  for tag in values_for_tag(&frame.categories, "_item.name") {
    items.insert(
      tag.to_owned(),
      DictionaryItem {
        tag: tag.to_owned(),
        type_code: type_code.clone(),
        parent: parent.clone(),
      },
    );
  }
}

/// Resolves a primitive type through dictionary parent links.
fn resolve_type(item: &DictionaryItem, items: &BTreeMap<String, DictionaryItem>) -> Result<String, GeneratorError> {
  let mut current = item;
  let mut visited = BTreeSet::new();
  loop {
    if let Some(type_code) = &current.type_code {
      return Ok(type_code.clone());
    }
    if !visited.insert(current.tag.as_str()) {
      return Err(GeneratorError::MissingItemType(item.tag.clone()));
    }
    let Some(parent) = current.parent.as_deref().and_then(|parent| items.get(parent)) else {
      return Err(GeneratorError::MissingItemType(item.tag.clone()));
    };
    current = parent;
  }
}

/// Iterates concrete text values for a scalar or loop dictionary tag.
fn values_for_tag<'a>(categories: &'a [CifCategory], tag: &'a str) -> impl Iterator<Item = &'a str> {
  categories.iter().flat_map(move |category| match category {
    CifCategory::Item { tag: candidate, value } if candidate == tag => value.as_text().into_iter().collect(),
    CifCategory::Loop { tags, rows } => tags
      .iter()
      .position(|candidate| candidate == tag)
      .into_iter()
      .flat_map(|index| {
        rows
          .iter()
          .filter_map(move |row| row.get(index).and_then(|value| value.as_text()))
      })
      .collect(),
    CifCategory::Item { .. } => Vec::new(),
  })
}

/// Returns the category component of a canonical `_category.item` tag.
fn category_name(tag: &str) -> Option<&str> {
  tag.strip_prefix('_')?.split_once('.').map(|(category, _)| category)
}

/// Renders stable, rustfmt-friendly schema declarations.
fn render_schema(schema: &Schema) -> Result<String, GeneratorError> {
  let mut output = String::new();
  writeln!(output, "//! Generated typed views of selected PDBx/mmCIF categories.")?;
  writeln!(output, "//!")?;
  writeln!(output, "//! Dictionary version: {}.", schema.version)?;
  writeln!(
    output,
    "//! Generated by `just generate-mmcif-schema`; do not edit manually.\n"
  )?;
  writeln!(output, "use chitin_bio_macros::mmcif_category;\n")?;

  for (category_name, category) in &schema.categories {
    let type_name = rust_type_name(category_name);
    writeln!(output, "/// Typed schema for the `_{category_name}` category.")?;
    writeln!(output, "#[mmcif_category(name = \"{category_name}\")]")?;
    writeln!(output, "pub(crate) struct {type_name} {{")?;
    let mut identifiers = BTreeMap::<String, String>::new();
    for item in category.items.values() {
      let item_name = item.tag.split_once('.').map_or(item.tag.as_str(), |(_, name)| name);
      let identifier = rust_identifier(item_name);
      if let Some(first) = identifiers.insert(identifier.clone(), item.tag.clone()) {
        return Err(GeneratorError::IdentifierCollision {
          category: category_name.clone(),
          first,
          second: item.tag.clone(),
          identifier,
        });
      }
      writeln!(output, "  #[mmcif(tag = \"{}\")]", item.tag)?;
      writeln!(output, "  pub(crate) {identifier}: {},", marker_type(&item.type_code))?;
    }
    writeln!(output, "}}\n")?;
  }
  Ok(output)
}

/// Converts a dictionary category name to an idiomatic Rust type name.
fn rust_type_name(value: &str) -> String {
  value
    .split('_')
    .filter(|part| !part.is_empty())
    .map(|part| {
      let mut characters = part.chars();
      characters
        .next()
        .map(|first| first.to_ascii_uppercase().to_string() + characters.as_str())
        .unwrap_or_default()
    })
    .collect()
}

/// Converts a dictionary item name to a stable snake-case Rust identifier.
fn rust_identifier(value: &str) -> String {
  let mut identifier = String::new();
  let mut previous_underscore = false;
  for character in value.chars() {
    let normalized = if character.is_ascii_alphanumeric() {
      character.to_ascii_lowercase()
    } else {
      '_'
    };
    if normalized == '_' && previous_underscore {
      continue;
    }
    identifier.push(normalized);
    previous_underscore = normalized == '_';
  }
  let identifier = identifier.trim_matches('_');
  let mut identifier = if identifier
    .chars()
    .next()
    .is_some_and(|character| character.is_ascii_digit())
  {
    format!("item_{identifier}")
  } else {
    identifier.to_owned()
  };
  if is_rust_keyword(&identifier) {
    identifier.push('_');
  }
  identifier
}

/// Maps dictionary primitive types onto schema conversion markers.
fn marker_type(type_code: &str) -> &'static str {
  match type_code {
    "float" | "float-range" => "Float",
    "int" | "int-range" | "positive_int" => "Integer",
    "boolean" => "Boolean",
    "uchar1" => "Character",
    _ => "Text",
  }
}

/// Reports whether an identifier is reserved by Rust 2024.
fn is_rust_keyword(value: &str) -> bool {
  matches!(
    value,
    "as"
      | "async"
      | "await"
      | "break"
      | "const"
      | "continue"
      | "crate"
      | "dyn"
      | "else"
      | "enum"
      | "extern"
      | "false"
      | "fn"
      | "for"
      | "gen"
      | "if"
      | "impl"
      | "in"
      | "let"
      | "loop"
      | "match"
      | "mod"
      | "move"
      | "mut"
      | "pub"
      | "ref"
      | "return"
      | "self"
      | "Self"
      | "static"
      | "struct"
      | "super"
      | "trait"
      | "true"
      | "type"
      | "unsafe"
      | "use"
      | "where"
      | "while"
  )
}

#[cfg(test)]
mod tests {
  use std::collections::BTreeSet;

  use chitin_bio::structure::cif::CifParser;

  use super::{extract_schema, marker_type, parse_selection, render_schema, rust_identifier, rust_type_name};

  #[test]
  fn selection_should_ignore_comments_and_blank_lines() {
    assert_eq!(parse_selection("# selected\natom_site\n\nentity_poly\n").len(), 2);
  }

  #[test]
  fn identifier_should_normalize_dictionary_punctuation() {
    assert_eq!(rust_identifier("aniso_B[1][1]"), "aniso_b_1_1");
  }

  #[test]
  fn type_name_should_use_pascal_case() {
    assert_eq!(rust_type_name("entity_poly_seq"), "EntityPolySeq");
  }

  #[test]
  fn marker_should_preserve_numeric_semantics() {
    assert_eq!(marker_type("positive_int"), "Integer");
  }

  #[test]
  fn linked_item_should_inherit_parent_type() {
    let document = CifParser::parse(
      r#"data_dictionary
_dictionary.version 1.0
save__base.id
loop_
_item.name
_item.category_id
'_base.id' base
'_child.id' child
loop_
_item_linked.child_name
_item_linked.parent_name
'_child.id' '_base.id'
_item_type.code int
save_
save__child.id
_item.name '_child.id'
save_
"#,
    )
    .unwrap_or_else(|error| panic!("dictionary fixture should parse: {error}"));
    let selected = BTreeSet::from(["child".to_owned()]);
    let schema =
      extract_schema(&document, &selected).unwrap_or_else(|error| panic!("linked type should resolve: {error}"));
    let rendered = render_schema(&schema).unwrap_or_else(|error| panic!("schema should render: {error}"));

    assert!(rendered.contains("pub(crate) id: Integer,"));
  }
}
