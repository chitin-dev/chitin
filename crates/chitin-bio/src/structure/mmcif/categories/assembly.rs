//! Biological assembly operations and generation rules from mmCIF.

use crate::structure::mmcif::cif::CifDocument;
use crate::structure::mmcif::schema::{PdbxStructAssembly, PdbxStructAssemblyGen, PdbxStructOperList};
use crate::structure::pdb::StructureBuilder;
use crate::structure::{AssemblyGeneration, BiologicalAssembly, MmcifParseError, StructureOperation};

use super::{map_builder_error, required};

/// Projects assembly operations and non-materialized generation rules.
///
/// The operation matrix and translation are stored exactly once. Generation
/// rows retain their raw operator expressions so a later assembly-expansion
/// pass can interpret ranges and products without duplicating coordinates while
/// parsing the source file.
///
/// # Parameters
///
/// * `document` contains the generic `_pdbx_struct_*` categories.
/// * `builder` receives operations and biological assembly definitions.
///
/// # Returns
///
/// `Ok(())` when the optional categories are absent or valid. Returns a field
/// error for incomplete rows and a structure error for duplicate or unknown
/// assembly references.
pub(crate) fn parse(document: &CifDocument, builder: &mut StructureBuilder) -> Result<(), MmcifParseError> {
  parse_operations(document, builder)?;
  parse_assemblies(document, builder)?;
  parse_generations(document, builder)
}

/// Reads rigid operations from `_pdbx_struct_oper_list`.
fn parse_operations(document: &CifDocument, builder: &mut StructureBuilder) -> Result<(), MmcifParseError> {
  let Some(category) = PdbxStructOperList::from_document(document) else {
    return Ok(());
  };
  for row in category.rows() {
    let operation = StructureOperation {
      id: required(row.row_number(), "_pdbx_struct_oper_list.id", row.id())?.to_owned(),
      rotation: [
        [
          required(
            row.row_number(),
            "_pdbx_struct_oper_list.matrix[1][1]",
            row.matrix_1_1()?,
          )?,
          required(
            row.row_number(),
            "_pdbx_struct_oper_list.matrix[1][2]",
            row.matrix_1_2()?,
          )?,
          required(
            row.row_number(),
            "_pdbx_struct_oper_list.matrix[1][3]",
            row.matrix_1_3()?,
          )?,
        ],
        [
          required(
            row.row_number(),
            "_pdbx_struct_oper_list.matrix[2][1]",
            row.matrix_2_1()?,
          )?,
          required(
            row.row_number(),
            "_pdbx_struct_oper_list.matrix[2][2]",
            row.matrix_2_2()?,
          )?,
          required(
            row.row_number(),
            "_pdbx_struct_oper_list.matrix[2][3]",
            row.matrix_2_3()?,
          )?,
        ],
        [
          required(
            row.row_number(),
            "_pdbx_struct_oper_list.matrix[3][1]",
            row.matrix_3_1()?,
          )?,
          required(
            row.row_number(),
            "_pdbx_struct_oper_list.matrix[3][2]",
            row.matrix_3_2()?,
          )?,
          required(
            row.row_number(),
            "_pdbx_struct_oper_list.matrix[3][3]",
            row.matrix_3_3()?,
          )?,
        ],
      ],
      translation: [
        required(row.row_number(), "_pdbx_struct_oper_list.vector[1]", row.vector_1()?)?,
        required(row.row_number(), "_pdbx_struct_oper_list.vector[2]", row.vector_2()?)?,
        required(row.row_number(), "_pdbx_struct_oper_list.vector[3]", row.vector_3()?)?,
      ],
    };
    builder.add_structure_operation(operation).map_err(map_builder_error)?;
  }
  Ok(())
}

/// Reads assembly identifiers and descriptions from `_pdbx_struct_assembly`.
fn parse_assemblies(document: &CifDocument, builder: &mut StructureBuilder) -> Result<(), MmcifParseError> {
  let Some(category) = PdbxStructAssembly::from_document(document) else {
    return Ok(());
  };
  for row in category.rows() {
    builder
      .add_biological_assembly(BiologicalAssembly {
        id: required(row.row_number(), "_pdbx_struct_assembly.id", row.id())?.to_owned(),
        details: row.details().map(str::to_owned),
        generations: Vec::new(),
      })
      .map_err(map_builder_error)?;
  }
  Ok(())
}

/// Reads chain selections and raw operator expressions for each assembly.
fn parse_generations(document: &CifDocument, builder: &mut StructureBuilder) -> Result<(), MmcifParseError> {
  let Some(category) = PdbxStructAssemblyGen::from_document(document) else {
    return Ok(());
  };
  for row in category.rows() {
    let assembly_id = required(
      row.row_number(),
      "_pdbx_struct_assembly_gen.assembly_id",
      row.assembly_id(),
    )?;
    let operator_expression = required(
      row.row_number(),
      "_pdbx_struct_assembly_gen.oper_expression",
      row.oper_expression(),
    )?;
    let asym_ids = split_identifier_list(row.asym_id_list());
    let auth_asym_ids = split_identifier_list(row.auth_asym_id_list());
    let entity_instance_ids = split_identifier_list(row.entity_inst_id());
    if asym_ids.is_empty() && auth_asym_ids.is_empty() && entity_instance_ids.is_empty() {
      return Err(MmcifParseError::InvalidField {
        row: row.row_number(),
        field: "_pdbx_struct_assembly_gen.asym_id_list",
        value: String::new(),
      });
    }
    builder
      .add_assembly_generation(
        assembly_id,
        AssemblyGeneration {
          asym_ids,
          auth_asym_ids,
          entity_instance_ids,
          operator_expression: operator_expression.to_owned(),
        },
      )
      .map_err(map_builder_error)?;
  }
  Ok(())
}

/// Splits a comma-separated asym identifier list while preserving source order.
fn split_identifier_list(value: Option<&str>) -> Vec<String> {
  value
    .into_iter()
    .flat_map(|value| value.split(','))
    .map(str::trim)
    .filter(|value| !value.is_empty())
    .map(str::to_owned)
    .collect()
}
