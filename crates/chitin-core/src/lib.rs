#![forbid(unsafe_code)]

pub mod workspace;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceSummary {
  pub product_name: &'static str,
  pub focus: &'static str,
}

impl Default for WorkspaceSummary {
  fn default() -> Self {
    Self {
      product_name: "Chitin",
      focus: "Computational chemistry and bioinformatics",
    }
  }
}
