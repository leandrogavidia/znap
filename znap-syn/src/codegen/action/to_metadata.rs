use crate::{ActionAttributesStruct, ActionLinkParameterStruct, ActionLinkStruct, ActionStruct, ActionLinkParameterTypeOption};
use proc_macro2::TokenStream;
use quote::quote;

fn generate_parameter_options(options: &[ActionLinkParameterTypeOption]) -> TokenStream {
    let options: Vec<_> = options
        .iter()
        .map(|o| {
            let label = &o.label;
            let value = &o.value;

            quote! {
                LinkedActionLinkParameterTypeOption {
                    label: #label.to_string(),
                    value: #value.to_string()
                }
            }
        })
        .collect();

    quote! {
        vec!(#(#options),*)
    }
}

fn generate_parameter(parameters: &[ActionLinkParameterStruct]) -> TokenStream {
    let parameters: Vec<_> = parameters
        .iter()
        .map(|p| {
            let label = &p.label;
            let name = &p.name;
            let required = p.required;
            let r#type = &p.r#type;
            let options = generate_parameter_options(&p.options);
 
            quote! {
                LinkedActionParameter {
                    label: #label.to_string(),
                    name: #name.to_string(),
                    required: #required,
                    r#type: #r#type.to_string(),
                    options: #options
                }
            }
        })
        .collect();
    quote! {
        vec!(#(#parameters),*)
    }
}

fn generate_links(links: &[ActionLinkStruct], route: &str) -> TokenStream {
    let links: Vec<_> = links
        .iter()
        .map(|l| {
            let label = &l.label;
            let href = if l.href.starts_with(route)
                || l.href.starts_with("http")
                || l.href.starts_with('/')
            {
                l.href.to_owned()
            } else {
                let route = route
                    .split('/')
                    .map(|r| {
                        if r.starts_with(':') {
                            return format!("{{{{params.{}}}}}", r.replace(':', ""));
                        }
                        r.to_owned()
                    })
                    .collect::<Vec<_>>()
                    .join("/");
                format!("{route}{}", l.href)
            };
            let params = generate_parameter(&l.parameters);

            quote! {
                LinkedAction {
                    label: #label.to_string(),
                    href: #href.to_string(),
                    parameters: #params,
                }
            }
        })
        .collect();

    if links.is_empty() {
        quote! {
            None
        }
    } else {
        quote! {
            Some(
                ActionLinks {
                    actions: vec!(#(#links),*)
                }
            )
        }
    }
}

pub fn generate(action_struct: &ActionStruct, route: &str) -> TokenStream {
    let name = &action_struct.name;

    if let Some(action_attributes) = &action_struct.attributes {
        let ActionAttributesStruct {
            title,
            description,
            label,
            icon,
            links,
            ..
        } = action_attributes;

        let links = generate_links(links, route);

        quote! {
            impl ToMetadata for #name {
                fn to_metadata() -> ActionMetadata {
                    ActionMetadata {
                        icon: #icon.to_string(),
                        title: #title.to_string(),
                        description: #description.to_string(),
                        label: #label.to_string(),
                        links: #links,
                        disabled: false,
                        error: None,
                    }
                }
            }
        }
    } else {
        quote! {
            impl ToMetadata for #name {
                fn to_metadata() -> ActionMetadata {
                    ActionMetadata {
                        icon: "".to_string(),
                        title: "".to_string(),
                        description: "".to_string(),
                        label: "".to_string(),
                        links: Some(
                            ActionLinks {
                                actions: vec!()
                            }
                        ),
                        disabled: false,
                        error: None,
                    }
                }
            }
        }
    }
}
