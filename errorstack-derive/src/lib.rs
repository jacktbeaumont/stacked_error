use heck::ToSnakeCase;
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{Data, DeriveInput, Field, Fields, Ident};

/// Derive macro for [`ErrorStack`].
///
/// Supports enums and structs with named fields. Note that the type must
/// also implement [`Display`](std::fmt::Display) and
/// [`Error`](std::error::Error). This can be accomplished manually or via
/// [`thiserror`](https://crates.io/crates/thiserror).
///
/// This macro implements [`ErrorStack`] according to field names and
/// attributes, and generates an ergonomic constructor for each struct or
/// enum variant that captures caller location via `#[track_caller]` and
/// composes naturally with [`Result::map_err`] for error chaining.
///
/// # Attributes
///
/// The following field attributes are available:
///
/// | Attribute         | Effect                                                                    | Auto-detected |
/// |-------------------|---------------------------------------------------------------------------|---------------|
/// | `#[source]`       | Marks a field as the error source.                                        | when field is named `source` |
/// | `#[stack_source]` | Marks the field as both the error source and an [`ErrorStack`] implementor, enabling typed chain walking via `ErrorStack::stack_source`. Implies `#[source]`. | no |
/// | `#[location]`     | Indicates the field stores a `&'static Location<'static>`, captured automatically at construction time. | no |
///
/// These attributes follow the same field conventions as
/// [`thiserror`](https://crates.io/crates/thiserror), allowing
/// both crates to be ergonomically used together.
///
/// # Stack sources
///
/// Any source field that implements [`ErrorStack`] should be annotated with
/// `#[stack_source]` to preserve the typed error chain. The macro cannot
/// inspect trait implementations, so without this annotation the source is
/// treated as a plain [`std::error::Error`] and chain walking stops at that
/// field.
///
/// # Optional sources
///
/// A source field may be wrapped in [`Option`] to represent errors that
/// do not always have an underlying cause. When the macro detects an
/// `Option<T>` source field, it generates two constructors instead of
/// one:
///
/// | Constructor | Signature | Source value |
/// |-------------|-----------|-------------|
/// | `variant_name` / `new` | `(user_fields…) -> Self` | `None` |
/// | `variant_name_with` / `new_with` | `(user_fields…) -> impl FnOnce(T) -> Self` | `Some(source)` |
///
/// # Error constructors
///
/// This macro also generates helper constructors for each struct or enum
/// variant. Every constructor is marked `#[track_caller]`, so the
/// call-site location is recorded without manual boilerplate. When a
/// source field is present alongside user fields, the constructor returns
/// `impl FnOnce(SourceTy) -> Self`, capturing the user fields and
/// composing directly with [`Result::map_err`]. When the source is the
/// only field, the constructor takes it as a parameter and returns `Self`
/// directly.
///
/// Constructors are `pub(crate)` and named `new` for structs or
/// `snake_cased_variant` for enum variants. Remaining fields
/// become parameters, while `#[source]` and `#[location]` fields are filled
/// automatically.
///
/// # Examples
///
/// The macro may be derived on enums and structs with named fields. This
/// example shows both, with `thiserror` compatibility.
///
/// ```
/// # use errorstack::ErrorStack;
/// #[derive(thiserror::Error, ErrorStack, Debug)]
/// pub enum AppError {
///     #[error("io failed: {path}")]
///     Io {
///         path: String,
///         source: std::io::Error,
///         #[location]
///         location: &'static std::panic::Location<'static>,
///     },
///
///     #[error("inner failed")]
///     Inner {
///         #[stack_source]
///         source: ConfigError,
///         #[location]
///         location: &'static std::panic::Location<'static>,
///     },
///
///     #[error("not found: {id}")]
///     NotFound {
///         id: String,
///         #[location]
///         location: &'static std::panic::Location<'static>,
///     },
/// }
///
/// #[derive(thiserror::Error, ErrorStack, Debug)]
/// #[error("config: {detail}")]
/// pub struct ConfigError {
///     detail: String,
///     #[location]
///     location: &'static std::panic::Location<'static>,
/// }
/// ```
///
/// The derive above generates the following constructors:
///
/// ```text
/// impl AppError {
///     // Source variants return a closure for use with map_err.
///     pub(crate) fn io(path: String) -> impl FnOnce(io::Error) -> Self;
///     pub(crate) fn inner(source: ConfigError) -> Self;
///     // Sourceless variants return Self directly.
///     pub(crate) fn not_found(id: String) -> Self;
/// }
///
/// impl ConfigError {
///     pub(crate) fn new(detail: String) -> Self;
/// }
/// ```
///
/// Source and location fields are handled automatically by these
/// constructors, keeping call sites concise:
///
/// ```
/// # use errorstack::ErrorStack;
/// # #[derive(thiserror::Error, ErrorStack, Debug)]
/// # pub enum AppError {
/// #     #[error("io failed: {path}")]
/// #     Io {
/// #         path: String,
/// #         source: std::io::Error,
/// #         #[location]
/// #         location: &'static std::panic::Location<'static>,
/// #     },
/// #     #[error("not found: {id}")]
/// #     NotFound {
/// #         id: String,
/// #         #[location]
/// #         location: &'static std::panic::Location<'static>,
/// #     },
/// # }
/// # fn main() -> Result<(), AppError> {
/// let _content = std::fs::read_to_string("Cargo.toml")
///     .map_err(AppError::io("Cargo.toml".into()))?;
///
/// let _err = AppError::not_found("abc".into());
/// # Ok(())
/// # }
/// ```
#[proc_macro_derive(ErrorStack, attributes(source, stack_source, location))]
pub fn derive_error_stack(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as DeriveInput);
    match derive_impl(input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn derive_impl(input: DeriveInput) -> syn::Result<TokenStream2> {
    let name = &input.ident;

    match &input.data {
        Data::Enum(data) => {
            let mut constructor_methods = Vec::new();
            let mut location_arms = Vec::new();
            let mut stack_source_arms = Vec::new();

            for variant in &data.variants {
                let variant_name = &variant.ident;
                let fields = match &variant.fields {
                    Fields::Named(f) => f,
                    Fields::Unnamed(_) => {
                        return Err(syn::Error::new(
                            variant_name.span(),
                            format!(
                                "ErrorStack derive requires named (struct) variants; \
                                 found tuple variant `{variant_name}`"
                            ),
                        ));
                    }
                    Fields::Unit => {
                        return Err(syn::Error::new(
                            variant_name.span(),
                            format!(
                                "ErrorStack derive requires named (struct) variants; \
                                 found unit variant `{variant_name}`"
                            ),
                        ));
                    }
                };

                let parsed = parse_fields(&fields.named, variant_name)?;

                constructor_methods.push(gen_constructor_enum(variant_name, &parsed));
                location_arms.push(gen_location_arm_enum(variant_name, &parsed));
                stack_source_arms.push(gen_stack_source_arm_enum(variant_name, &parsed));
            }

            Ok(quote! {
                impl #name {
                    #(#constructor_methods)*
                }

                impl ::errorstack::ErrorStack for #name {
                    fn location(&self) -> Option<&'static ::std::panic::Location<'static>> {
                        match self {
                            #(#location_arms)*
                        }
                    }

                    fn stack_source(&self) -> Option<&dyn ::errorstack::ErrorStack> {
                        match self {
                            #(#stack_source_arms)*
                        }
                    }
                }
            })
        }

        Data::Struct(data) => {
            let fields = match &data.fields {
                Fields::Named(f) => f,
                _ => {
                    return Err(syn::Error::new(
                        name.span(),
                        "ErrorStack derive requires named fields",
                    ));
                }
            };

            let parsed = parse_fields(&fields.named, name)?;
            let constructor = gen_constructor_struct(name, &parsed);

            let location_body = if let Some(loc) = &parsed.location {
                let loc_ident = &loc.ident;
                quote! { Some(self.#loc_ident) }
            } else {
                quote! { None }
            };

            let stack_source_body = if parsed.stack_source {
                let src = parsed.source.as_ref().unwrap();
                let src_ident = &src.ident;
                if parsed.optional_source {
                    quote! { self.#src_ident.as_ref().map(|s| s as &dyn ::errorstack::ErrorStack) }
                } else {
                    quote! { Some(&self.#src_ident as &dyn ::errorstack::ErrorStack) }
                }
            } else {
                quote! { None }
            };

            Ok(quote! {
                impl #name {
                    #constructor
                }

                impl ::errorstack::ErrorStack for #name {
                    fn location(&self) -> Option<&'static ::std::panic::Location<'static>> {
                        #location_body
                    }

                    fn stack_source(&self) -> Option<&dyn ::errorstack::ErrorStack> {
                        #stack_source_body
                    }
                }
            })
        }

        Data::Union(_) => Err(syn::Error::new(
            name.span(),
            "ErrorStack derive is not supported on unions",
        )),
    }
}

struct ParsedFields<'a> {
    source: Option<&'a Field>,
    location: Option<&'a Field>,
    stack_source: bool,
    optional_source: bool,
    /// The inner type `T` when source is `Option<T>`.
    inner_source_ty: Option<syn::Type>,
    user_fields: Vec<&'a Field>,
}

fn attr(field: &Field, name: &str) -> bool {
    field.attrs.iter().any(|a| a.path().is_ident(name))
}

/// If `ty` is `Option<T>`, returns the inner type `T`.
fn extract_option_inner(ty: &syn::Type) -> Option<&syn::Type> {
    let syn::Type::Path(type_path) = ty else {
        return None;
    };
    let segment = type_path.path.segments.last()?;
    if segment.ident != "Option" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };
    if args.args.len() != 1 {
        return None;
    }
    let syn::GenericArgument::Type(inner) = args.args.first()? else {
        return None;
    };
    Some(inner)
}

fn parse_fields<'a>(
    fields: &'a syn::punctuated::Punctuated<Field, syn::Token![,]>,
    context_name: &Ident,
) -> syn::Result<ParsedFields<'a>> {
    let mut source: Option<&Field> = None;
    let mut location: Option<&Field> = None;
    let mut stack_source = false;
    let mut optional_source = false;
    let mut inner_source_ty = None;
    let mut user_fields = Vec::new();

    for field in fields {
        let ident = field.ident.as_ref().unwrap();
        let source_by_name = ident == "source";
        let source_by_attr = attr(field, "source");
        let location_attr = attr(field, "location");
        let stack_source_attr = attr(field, "stack_source");

        if source_by_name || source_by_attr || stack_source_attr {
            if source.is_some() {
                return Err(syn::Error::new(
                    ident.span(),
                    format!("variant `{context_name}` has multiple source fields"),
                ));
            }
            source = Some(field);
            if stack_source_attr {
                stack_source = true;
            }
            if let Some(inner) = extract_option_inner(&field.ty) {
                optional_source = true;
                inner_source_ty = Some(inner.clone());
            }
        } else if location_attr {
            if location.is_some() {
                return Err(syn::Error::new(
                    ident.span(),
                    format!("variant `{context_name}` has multiple location fields"),
                ));
            }
            location = Some(field);
        } else {
            user_fields.push(field);
        }
    }

    Ok(ParsedFields {
        source,
        location,
        stack_source,
        optional_source,
        inner_source_ty,
        user_fields,
    })
}

/// Names and self-expression that vary between enum variants and structs.
struct ConstructorCtx {
    method_name: Ident,
    with_method_name: Ident,
    doc: String,
    doc_with: String,
    /// Token stream for constructing the type, e.g. `Self::Variant` or `Self`.
    self_path: TokenStream2,
}

fn gen_constructor(ctx: &ConstructorCtx, parsed: &ParsedFields<'_>) -> TokenStream2 {
    let ConstructorCtx {
        method_name,
        with_method_name,
        doc,
        doc_with,
        self_path,
    } = ctx;

    let user_params: Vec<_> = parsed
        .user_fields
        .iter()
        .map(|f| {
            let ident = &f.ident;
            let ty = &f.ty;
            quote! { #ident: #ty }
        })
        .collect();

    let user_field_names: Vec<_> = parsed.user_fields.iter().map(|f| &f.ident).collect();

    let location_init = parsed.location.as_ref().map(|f| {
        let ident = &f.ident;
        quote! { #ident: location, }
    });

    let location_capture = parsed.location.as_ref().map(|_| {
        quote! { let location = ::std::panic::Location::caller(); }
    });

    if let Some(src) = &parsed.source {
        let src_ident = &src.ident;

        if parsed.optional_source {
            let inner_ty = parsed.inner_source_ty.as_ref().unwrap();
            quote! {
                #[doc = #doc]
                #[track_caller]
                pub(crate) fn #method_name(#(#user_params),*) -> Self {
                    #location_capture
                    #self_path {
                        #src_ident: None,
                        #(#user_field_names,)*
                        #location_init
                    }
                }

                #[doc = #doc_with]
                #[track_caller]
                pub(crate) fn #with_method_name(#(#user_params),*) -> impl ::std::ops::FnOnce(#inner_ty) -> Self {
                    #location_capture
                    move |#src_ident| #self_path {
                        #src_ident: Some(#src_ident),
                        #(#user_field_names,)*
                        #location_init
                    }
                }
            }
        } else if parsed.user_fields.is_empty() {
            let src_ty = &src.ty;
            quote! {
                #[doc = #doc]
                #[track_caller]
                pub(crate) fn #method_name(#src_ident: #src_ty) -> Self {
                    #location_capture
                    #self_path {
                        #src_ident,
                        #location_init
                    }
                }
            }
        } else {
            let src_ty = &src.ty;
            quote! {
                #[doc = #doc]
                #[track_caller]
                pub(crate) fn #method_name(#(#user_params),*) -> impl ::std::ops::FnOnce(#src_ty) -> Self {
                    #location_capture
                    move |#src_ident| #self_path {
                        #src_ident,
                        #(#user_field_names,)*
                        #location_init
                    }
                }
            }
        }
    } else {
        quote! {
            #[doc = #doc]
            #[track_caller]
            pub(crate) fn #method_name(#(#user_params),*) -> Self {
                #location_capture
                #self_path {
                    #(#user_field_names,)*
                    #location_init
                }
            }
        }
    }
}

fn gen_constructor_enum(variant_name: &Ident, parsed: &ParsedFields<'_>) -> TokenStream2 {
    let snake = variant_name.to_string().to_snake_case();
    let ctx = ConstructorCtx {
        method_name: Ident::new(&snake, variant_name.span()),
        with_method_name: Ident::new(&format!("{snake}_with"), variant_name.span()),
        doc: format!("Constructs a [`{variant_name}`](Self::{variant_name}) error."),
        doc_with: format!(
            "Constructs a [`{variant_name}`](Self::{variant_name}) error with a source."
        ),
        self_path: quote! { Self::#variant_name },
    };
    gen_constructor(&ctx, parsed)
}

fn gen_constructor_struct(type_name: &Ident, parsed: &ParsedFields<'_>) -> TokenStream2 {
    let ctx = ConstructorCtx {
        method_name: Ident::new("new", type_name.span()),
        with_method_name: Ident::new("new_with", type_name.span()),
        doc: format!("Constructs a [`{type_name}`]."),
        doc_with: format!("Constructs a [`{type_name}`] with a source."),
        self_path: quote! { Self },
    };
    gen_constructor(&ctx, parsed)
}

fn gen_location_arm_enum(variant_name: &Ident, parsed: &ParsedFields<'_>) -> TokenStream2 {
    if let Some(loc) = &parsed.location {
        let loc_ident = &loc.ident;
        quote! {
            Self::#variant_name { #loc_ident, .. } => Some(#loc_ident),
        }
    } else {
        quote! {
            Self::#variant_name { .. } => None,
        }
    }
}

fn gen_stack_source_arm_enum(variant_name: &Ident, parsed: &ParsedFields<'_>) -> TokenStream2 {
    if parsed.stack_source {
        let src_ident = &parsed.source.unwrap().ident;
        if parsed.optional_source {
            quote! {
                Self::#variant_name { #src_ident, .. } => #src_ident.as_ref().map(|s| s as &dyn ::errorstack::ErrorStack),
            }
        } else {
            quote! {
                Self::#variant_name { #src_ident, .. } => Some(#src_ident as &dyn ::errorstack::ErrorStack),
            }
        }
    } else {
        quote! {
            Self::#variant_name { .. } => None,
        }
    }
}
