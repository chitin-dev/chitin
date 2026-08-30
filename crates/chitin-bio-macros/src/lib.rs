#![forbid(unsafe_code)]
//! Procedural macros for generated Chitin biological format schemas.

use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Attribute, Error, Fields, Ident, ItemStruct, LitStr, Result, Token, Type, Visibility, parse_macro_input};

/// Generates a typed borrowed category and its item accessors.
///
/// The annotated structure is a schema declaration consumed during macro
/// expansion. Its fields are not stored at runtime.
#[proc_macro_attribute]
pub fn mmcif_category(arguments: TokenStream, input: TokenStream) -> TokenStream {
  let arguments = parse_macro_input!(arguments as CategoryArguments);
  let declaration = parse_macro_input!(input as ItemStruct);
  expand_category(arguments, declaration)
    .unwrap_or_else(Error::into_compile_error)
    .into()
}

/// Parsed arguments of `#[mmcif_category(...)]`.
struct CategoryArguments {
  name: LitStr,
}

impl Parse for CategoryArguments {
  fn parse(input: ParseStream<'_>) -> Result<Self> {
    let key: Ident = input.parse()?;
    if key != "name" {
      return Err(Error::new(key.span(), "expected `name`"));
    }
    input.parse::<Token![=]>()?;
    let name = input.parse()?;
    if !input.is_empty() {
      return Err(input.error("unexpected category arguments"));
    }
    Ok(Self { name })
  }
}

/// One generated item accessor declaration.
struct ItemDeclaration {
  attributes: Vec<Attribute>,
  visibility: Visibility,
  method: Ident,
  tag: LitStr,
  kind: ItemKind,
}

/// Runtime conversion selected from an mmCIF dictionary type code.
enum ItemKind {
  Text,
  Integer,
  Float,
  Boolean,
  Character,
}

/// Expands one readable schema structure into zero-sized typed category APIs.
///
/// # Parameters
///
/// * `arguments` supplies the dictionary category name.
/// * `declaration` contains generated field names, tags, and conversion kinds.
///
/// # Returns
///
/// Rust items implementing the typed category and row accessors, or a compile
/// error for an unsupported declaration.
fn expand_category(arguments: CategoryArguments, declaration: ItemStruct) -> Result<proc_macro2::TokenStream> {
  let ItemStruct {
    attrs,
    vis,
    ident,
    fields,
    ..
  } = declaration;
  let Fields::Named(fields) = fields else {
    return Err(Error::new_spanned(ident, "mmCIF categories require named fields"));
  };
  let items = fields
    .named
    .into_iter()
    .map(parse_item_declaration)
    .collect::<Result<Vec<_>>>()?;
  let category_name = arguments.name;
  let accessors = items.iter().map(expand_accessor);

  Ok(quote! {
    #(#attrs)*
    #[derive(Debug, Clone, Copy)]
    #vis struct #ident;

    impl #ident {
      #[doc = concat!("Finds the `_", #category_name, "` category in a generic CIF document.")]
      #vis fn from_document<'a>(
        document: &'a crate::structure::mmcif::cif::CifDocument,
      ) -> Option<crate::structure::mmcif::category::TypedCategory<'a, Self>> {
        crate::structure::mmcif::category::TypedCategory::from_document(document, #category_name)
      }
    }

    impl<'a> crate::structure::mmcif::category::TypedRow<'a, #ident> {
      #(#accessors)*
    }
  })
}

/// Parses a generated schema field and its dictionary tag attribute.
fn parse_item_declaration(field: syn::Field) -> Result<ItemDeclaration> {
  let method = field
    .ident
    .ok_or_else(|| Error::new_spanned(&field.ty, "mmCIF schema fields must be named"))?;
  let tag = parse_tag(&field.attrs)?;
  let attributes = field
    .attrs
    .into_iter()
    .filter(|attribute| !attribute.path().is_ident("mmcif"))
    .collect();
  Ok(ItemDeclaration {
    attributes,
    visibility: field.vis,
    method,
    tag,
    kind: ItemKind::from_type(&field.ty)?,
  })
}

/// Reads `#[mmcif(tag = "...")]` from one generated schema field.
fn parse_tag(attributes: &[Attribute]) -> Result<LitStr> {
  let mut tag = None;
  for attribute in attributes.iter().filter(|attribute| attribute.path().is_ident("mmcif")) {
    attribute.parse_nested_meta(|meta| {
      if meta.path.is_ident("tag") {
        tag = Some(meta.value()?.parse()?);
        Ok(())
      } else {
        Err(meta.error("unsupported mmCIF field attribute"))
      }
    })?;
  }
  tag.ok_or_else(|| Error::new(proc_macro2::Span::call_site(), "missing `#[mmcif(tag = \"...\")]`"))
}

impl ItemKind {
  /// Maps a generated marker type onto one runtime conversion.
  fn from_type(value: &Type) -> Result<Self> {
    let Type::Path(path) = value else {
      return Err(Error::new_spanned(value, "expected a schema marker type"));
    };
    let Some(segment) = path.path.segments.last() else {
      return Err(Error::new_spanned(value, "expected a schema marker type"));
    };
    match segment.ident.to_string().as_str() {
      "Text" => Ok(Self::Text),
      "Integer" => Ok(Self::Integer),
      "Float" => Ok(Self::Float),
      "Boolean" => Ok(Self::Boolean),
      "Character" => Ok(Self::Character),
      _ => Err(Error::new_spanned(
        value,
        "expected Text, Integer, Float, Boolean, or Character",
      )),
    }
  }
}

/// Emits one typed getter backed by the generic category row.
fn expand_accessor(item: &ItemDeclaration) -> proc_macro2::TokenStream {
  let ItemDeclaration {
    attributes,
    visibility,
    method,
    tag,
    kind,
  } = item;
  match kind {
    ItemKind::Text => quote! {
      #(#attributes)*
      #[doc = concat!("Reads `", #tag, "`.")]
      #visibility fn #method(self) -> Option<&'a str> {
        self.raw.optional_text(&[#tag])
      }
    },
    ItemKind::Integer => quote! {
      #(#attributes)*
      #[doc = concat!("Reads `", #tag, "` as an optional integer.")]
      #visibility fn #method(self) -> Result<Option<i32>, crate::structure::MmcifParseError> {
        self.raw.optional_i32(&[#tag], #tag)
      }
    },
    ItemKind::Float => quote! {
      #(#attributes)*
      #[doc = concat!("Reads `", #tag, "` as an optional floating-point value.")]
      #visibility fn #method(self) -> Result<Option<f32>, crate::structure::MmcifParseError> {
        self.raw.optional_f32(&[#tag], #tag)
      }
    },
    ItemKind::Boolean => quote! {
      #(#attributes)*
      #[doc = concat!("Reads `", #tag, "` as an optional boolean.")]
      #visibility fn #method(self) -> Option<bool> {
        self.raw.optional_text(&[#tag]).map(|value| {
          value.eq_ignore_ascii_case("y") || value.eq_ignore_ascii_case("yes")
        })
      }
    },
    ItemKind::Character => quote! {
      #(#attributes)*
      #[doc = concat!("Reads `", #tag, "` as an optional character.")]
      #visibility fn #method(self) -> Option<char> {
        self.raw.optional_text(&[#tag]).and_then(|value| value.chars().next())
      }
    },
  }
}
