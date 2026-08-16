//! Secondary-structure ranges from `_struct_conf` and `_struct_sheet_range`.

use crate::structure::mmcif::cif::CifDocument;
use crate::structure::mmcif::schema::{StructConf, StructSheetRange};
use crate::structure::pdb::{PendingSecondaryRange, StructureBuilder};
use crate::structure::{MmcifParseError, SecondaryStructure};

/// Projects helix and sheet ranges into the shared structure builder.
///
/// # Parameters
///
/// * `document` contains generic mmCIF categories.
/// * `builder` receives deferred ranges resolved after residue construction.
///
/// # Returns
///
/// `Ok(())` when optional categories are absent or all present ranges are valid.
pub(crate) fn parse(document: &CifDocument, builder: &mut StructureBuilder) -> Result<(), MmcifParseError> {
  parse_helices(document, builder)?;
  parse_sheets(document, builder)
}

/// Adds supported `_struct_conf` helix variants.
fn parse_helices(document: &CifDocument, builder: &mut StructureBuilder) -> Result<(), MmcifParseError> {
  let Some(category) = StructConf::from_document(document) else {
    return Ok(());
  };
  for row in category.rows() {
    let Some(conf_type) = row.conf_type_id() else {
      continue;
    };
    let normalized = conf_type.to_ascii_uppercase();
    if !normalized.starts_with("HELX_") {
      continue;
    }
    let endpoints = range_endpoints(
      row.row_number(),
      "_struct_conf chain",
      EndpointNamespace {
        begin_chain: row.beg_auth_asym_id(),
        begin_sequence: row.beg_auth_seq_id().map(SequenceValue::Text),
        end_chain: row.end_auth_asym_id(),
        end_sequence: row.end_auth_seq_id().map(SequenceValue::Text),
      },
      EndpointNamespace {
        begin_chain: row.beg_label_asym_id(),
        begin_sequence: row.beg_label_seq_id()?.map(SequenceValue::Integer),
        end_chain: row.end_label_asym_id(),
        end_sequence: row.end_label_seq_id()?.map(SequenceValue::Integer),
      },
    )?;
    add_range(
      builder,
      row.row_number(),
      endpoints,
      row.pdbx_beg_pdb_ins_code().and_then(|value| value.chars().next()),
      row.pdbx_end_pdb_ins_code().and_then(|value| value.chars().next()),
      match normalized.as_str() {
        "HELX_RH_3T_P" => SecondaryStructure::Helix310,
        "HELX_RH_PI_P" => SecondaryStructure::PiHelix,
        _ => SecondaryStructure::Helix,
      },
    );
  }
  Ok(())
}

/// Adds `_struct_sheet_range` annotations.
fn parse_sheets(document: &CifDocument, builder: &mut StructureBuilder) -> Result<(), MmcifParseError> {
  let Some(category) = StructSheetRange::from_document(document) else {
    return Ok(());
  };
  for row in category.rows() {
    let endpoints = range_endpoints(
      row.row_number(),
      "_struct_sheet_range chain",
      EndpointNamespace {
        begin_chain: row.beg_auth_asym_id(),
        begin_sequence: row.beg_auth_seq_id().map(SequenceValue::Text),
        end_chain: row.end_auth_asym_id(),
        end_sequence: row.end_auth_seq_id().map(SequenceValue::Text),
      },
      EndpointNamespace {
        begin_chain: row.beg_label_asym_id(),
        begin_sequence: row.beg_label_seq_id()?.map(SequenceValue::Integer),
        end_chain: row.end_label_asym_id(),
        end_sequence: row.end_label_seq_id()?.map(SequenceValue::Integer),
      },
    )?;
    add_range(
      builder,
      row.row_number(),
      endpoints,
      row.pdbx_beg_pdb_ins_code().and_then(|value| value.chars().next()),
      row.pdbx_end_pdb_ins_code().and_then(|value| value.chars().next()),
      SecondaryStructure::Sheet,
    );
  }
  Ok(())
}

/// A complete pair of range endpoints from one identifier namespace.
#[derive(Debug, Clone, Copy)]
struct EndpointNamespace<'a> {
  begin_chain: Option<&'a str>,
  begin_sequence: Option<SequenceValue<'a>>,
  end_chain: Option<&'a str>,
  end_sequence: Option<SequenceValue<'a>>,
}

/// A sequence identifier read from a text-like author or integer label item.
#[derive(Debug, Clone, Copy)]
enum SequenceValue<'a> {
  Text(&'a str),
  Integer(i32),
}

impl EndpointNamespace<'_> {
  /// Converts this candidate into complete endpoints when no component is missing.
  fn complete(self, row: usize) -> Result<Option<RangeEndpoints>, MmcifParseError> {
    let (Some(begin_chain), Some(begin_sequence), Some(end_chain), Some(end_sequence)) =
      (self.begin_chain, self.begin_sequence, self.end_chain, self.end_sequence)
    else {
      return Ok(None);
    };
    Ok(Some(RangeEndpoints {
      begin_chain: begin_chain.to_owned(),
      begin_sequence: begin_sequence.parse(row, "secondary-structure start sequence")?,
      end_chain: end_chain.to_owned(),
      end_sequence: end_sequence.parse(row, "secondary-structure end sequence")?,
    }))
  }
}

/// Owned endpoints ready to outlive the borrowed category row.
#[derive(Debug)]
struct RangeEndpoints {
  begin_chain: String,
  begin_sequence: i32,
  end_chain: String,
  end_sequence: i32,
}

/// Selects a complete author namespace, falling back to a complete label namespace.
fn range_endpoints(
  row: usize,
  field: &'static str,
  author: EndpointNamespace<'_>,
  label: EndpointNamespace<'_>,
) -> Result<RangeEndpoints, MmcifParseError> {
  let endpoints = if let Some(endpoints) = author.complete(row)? {
    endpoints
  } else if let Some(endpoints) = label.complete(row)? {
    endpoints
  } else {
    return Err(MmcifParseError::InvalidField {
      row,
      field: "secondary-structure endpoints",
      value: "missing complete author and label endpoints".to_owned(),
    });
  };
  if endpoints.begin_chain != endpoints.end_chain {
    return Err(MmcifParseError::InvalidField {
      row,
      field,
      value: format!("{:?} and {:?}", endpoints.begin_chain, endpoints.end_chain),
    });
  }
  Ok(endpoints)
}

impl SequenceValue<'_> {
  /// Converts the selected dictionary representation to a residue number.
  fn parse(self, row: usize, field: &'static str) -> Result<i32, MmcifParseError> {
    match self {
      Self::Integer(value) => Ok(value),
      Self::Text(value) => value.parse().map_err(|_| MmcifParseError::InvalidField {
        row,
        field,
        value: value.to_owned(),
      }),
    }
  }
}

/// Stores one normalized secondary-structure interval.
fn add_range(
  builder: &mut StructureBuilder,
  row: usize,
  endpoints: RangeEndpoints,
  begin_insertion: Option<char>,
  end_insertion: Option<char>,
  kind: SecondaryStructure,
) {
  builder.add_secondary_range(PendingSecondaryRange {
    line: row,
    chain_id: Some(endpoints.begin_chain),
    start_sequence_number: endpoints.begin_sequence,
    start_insertion_code: begin_insertion,
    end_sequence_number: endpoints.end_sequence,
    end_insertion_code: end_insertion,
    kind,
  });
}
