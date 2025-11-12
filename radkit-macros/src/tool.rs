use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse::Parse, parse::ParseStream, Ident, LitStr, Token};

/// Parsed tool macro attributes
pub struct ToolArgs {
    pub description: String,
}

impl Parse for ToolArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut description = None;

        while !input.is_empty() {
            let key: Ident = input.parse()?;
            let _: Token![=] = input.parse()?;

            match key.to_string().as_str() {
                "name" => {
                    return Err(syn::Error::new(
                        key.span(),
                        "The 'name' attribute is not supported. Use the function name as the tool name instead.",
                    ));
                }
                "description" => {
                    let value: LitStr = input.parse()?;
                    description = Some(value.value());
                }
                _ => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!("Unknown tool attribute: {key}"),
                    ));
                }
            }

            if input.peek(Token![,]) {
                let _: Token![,] = input.parse()?;
            }
        }

        let description = description
            .ok_or_else(|| syn::Error::new(input.span(), "Missing required field: description"))?;

        Ok(Self { description })
    }
}

/// Generate the tool implementation
#[allow(clippy::needless_pass_by_value)] // We consume args and clone item, this is correct for proc macros
pub fn generate_tool_impl(args: ToolArgs, item: TokenStream) -> TokenStream {
    // Parse function item
    let func: syn::ItemFn = match syn::parse2(item) {
        Ok(f) => f,
        Err(e) => return e.to_compile_error(),
    };

    // Validate function is async
    if func.sig.asyncness.is_none() {
        return syn::Error::new_spanned(&func.sig, "Tool function must be async")
            .to_compile_error();
    }

    // Extract function components
    let func_name = &func.sig.ident;
    let func_name_str = func_name.to_string();
    let tool_name = &func_name_str;
    let description = &args.description;
    let body = &func.block;
    let vis = &func.vis;
    let attrs = &func.attrs;
    let generics = &func.sig.generics;
    let where_clause = &func.sig.generics.where_clause;

    // Parse parameters (args struct + optional ToolContext)
    let (args_param, ctx_param) = match parse_parameters(&func.sig) {
        Ok(params) => params,
        Err(e) => return e.to_compile_error(),
    };

    // Extract args type for schema generation
    let args_type = match extract_type(&args_param) {
        Ok(ty) => ty,
        Err(e) => return e.to_compile_error(),
    };

    // Generate parameter bindings
    let param_bindings = generate_param_bindings(&args_param, ctx_param.as_ref());

    // Generate schema
    let schema_gen = quote! {
        ::schemars::schema_for!(#args_type).to_value()
    };

    // Generate a struct with the function name and implement BaseTool directly
    // Note: The original async fn is consumed, we generate a struct that implements BaseTool
    // Preserves visibility and attributes from the original function
    quote! {
        #[derive(Clone, Copy, Debug)]
        #[allow(non_camel_case_types)]
        #(#attrs)*
        #vis struct #func_name #generics #where_clause;

        #[cfg_attr(all(target_os = "wasi", target_env = "p1"), ::async_trait::async_trait(?Send))]
        #[cfg_attr(not(all(target_os = "wasi", target_env = "p1")), ::async_trait::async_trait)]
        impl #generics ::radkit::tools::BaseTool for #func_name #generics #where_clause {
            fn name(&self) -> &str {
                #tool_name
            }

            fn description(&self) -> &str {
                #description
            }

            fn declaration(&self) -> ::radkit::tools::FunctionDeclaration {
                ::radkit::tools::FunctionDeclaration::new(
                    #tool_name,
                    #description,
                    #schema_gen
                )
            }

            async fn run_async(
                &self,
                __args: ::std::collections::HashMap<::std::string::String, ::serde_json::Value>,
                __ctx: &::radkit::tools::ToolContext<'_>,
            ) -> ::radkit::tools::ToolResult {
                #param_bindings
                #body
            }
        }
    }
}

/// Parses and validates tool function parameters.
///
/// Extracts the required args struct parameter and optional `&ToolContext<'_>` parameter.
/// Validates that:
/// - At least one parameter exists (args struct)
/// - At most two parameters exist (args struct + optional `ToolContext`)
/// - If present, the second parameter is `&ToolContext<'_>`
///
/// # Returns
///
/// A tuple of `(args_param, optional_ctx_param)` on success.
///
/// # Errors
///
/// Returns an error if the parameter count or types are invalid.
fn parse_parameters(sig: &syn::Signature) -> syn::Result<(syn::FnArg, Option<syn::FnArg>)> {
    let mut params = sig.inputs.iter();

    // First parameter must be args struct
    let args_param = params
        .next()
        .ok_or_else(|| {
            syn::Error::new_spanned(
                sig,
                "Tool function must have at least one parameter (args struct)",
            )
        })?
        .clone();

    // Second parameter (if present) should be &ToolContext<'_>
    let ctx_param = params.next().cloned();
    if let Some(ref param) = ctx_param {
        if !is_tool_context_type(param) {
            return Err(syn::Error::new_spanned(
                param,
                "Second parameter must be &ToolContext<'_>",
            ));
        }
    }

    // No more parameters allowed
    if params.next().is_some() {
        return Err(syn::Error::new_spanned(
            sig,
            "Tool function can have at most 2 parameters (args struct + optional ToolContext)",
        ));
    }

    Ok((args_param, ctx_param))
}

/// Checks if a function parameter is of type `&ToolContext<'_>`.
///
/// Validates that the parameter is a reference to `ToolContext` from `radkit::tools`.
/// Accepts both the short form (`ToolContext`) when imported, and the fully-qualified
/// forms (`radkit::tools::ToolContext`, `tools::ToolContext`).
///
/// # Arguments
///
/// * `param` - The function parameter to check
///
/// # Returns
///
/// `true` if the parameter is `&ToolContext<'_>`, `false` otherwise.
fn is_tool_context_type(param: &syn::FnArg) -> bool {
    // Check if parameter type is &ToolContext<'_> from radkit::tools
    if let syn::FnArg::Typed(pat_type) = param {
        if let syn::Type::Reference(type_ref) = &*pat_type.ty {
            if let syn::Type::Path(type_path) = &*type_ref.elem {
                // Accept both "ToolContext" and "radkit::tools::ToolContext"
                // When the type is in scope, syn gives us just "ToolContext"
                // When fully qualified, we get the full path
                let path_str = type_path
                    .path
                    .segments
                    .iter()
                    .map(|s| s.ident.to_string())
                    .collect::<Vec<_>>()
                    .join("::");

                // Accept "ToolContext" (when imported) or "radkit::tools::ToolContext" (fully qualified)
                return path_str == "ToolContext"
                    || path_str == "radkit::tools::ToolContext"
                    || path_str == "tools::ToolContext";
            }
        }
    }
    false
}

/// Extracts the type from a function parameter.
///
/// # Arguments
///
/// * `param` - The function parameter to extract the type from
///
/// # Returns
///
/// A reference to the parameter's type on success.
///
/// # Errors
///
/// Returns an error if the parameter is not a typed parameter (e.g., `self`).
fn extract_type(param: &syn::FnArg) -> syn::Result<&syn::Type> {
    if let syn::FnArg::Typed(pat_type) = param {
        Ok(&pat_type.ty)
    } else {
        Err(syn::Error::new_spanned(param, "Expected typed parameter"))
    }
}

/// Generates parameter binding code for the tool function closure.
///
/// Creates the deserialization logic that converts the `HashMap<String, Value>` args
/// into the typed args struct, and binds the optional `ToolContext` parameter.
///
/// # Arguments
///
/// * `args_param` - The args struct parameter
/// * `ctx_param` - The optional `&ToolContext<'_>` parameter
///
/// # Returns
///
/// Token stream containing the binding statements to insert at the start of the closure.
fn generate_param_bindings(args_param: &syn::FnArg, ctx_param: Option<&syn::FnArg>) -> TokenStream {
    let args_binding = if let syn::FnArg::Typed(pat_type) = args_param {
        let pat = &pat_type.pat;
        let ty = &pat_type.ty;

        quote! {
            let #pat: #ty = match ::serde_json::from_value(
                ::serde_json::Value::Object(__args.into_iter().collect())
            ) {
                Ok(val) => val,
                Err(e) => {
                    return ::radkit::tools::ToolResult::error(
                        format!("Invalid arguments: {}", e)
                    );
                }
            };
        }
    } else {
        quote! {}
    };

    let ctx_binding = if let Some(syn::FnArg::Typed(pat_type)) = ctx_param {
        let pat = &pat_type.pat;
        quote! {
            let #pat = __ctx;
        }
    } else {
        quote! {}
    };

    quote! {
        #args_binding
        #ctx_binding
    }
}
