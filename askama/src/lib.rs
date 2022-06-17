//! Askama implements a type-safe compiler for Jinja-like templates.
//! It lets you write templates in a Jinja-like syntax,
//! which are linked to a `struct` defining the template context.
//! This is done using a custom derive implementation (implemented
//! in [`askama_derive`](https://crates.io/crates/askama_derive)).
//!
//! For feature highlights and a quick start, please review the
//! [README](https://github.com/djc/askama/blob/main/README.md).
//!
//! The primary documentation for this crate now lives in
//! [the book](https://djc.github.io/askama/).
//!
//! # Creating Askama templates
//!
//! An Askama template is a `struct` definition which provides the template
//! context combined with a UTF-8 encoded text file (or inline source, see
//! below). Askama can be used to generate any kind of text-based format.
//! The template file's extension may be used to provide content type hints.
//!
//! A template consists of **text contents**, which are passed through as-is,
//! **expressions**, which get replaced with content while being rendered, and
//! **tags**, which control the template's logic.
//! The template syntax is very similar to [Jinja](http://jinja.pocoo.org/),
//! as well as Jinja-derivatives like [Twig](http://twig.sensiolabs.org/) or
//! [Tera](https://github.com/Keats/tera).
//!
//! ## The `template()` attribute
//!
//! Askama works by generating one or more trait implementations for any
//! `struct` type decorated with the `#[derive(Template)]` attribute. The
//! code generation process takes some options that can be specified through
//! the `template()` attribute. The following sub-attributes are currently
//! recognized:
//!
//! * `path` (as `path = "foo.html"`): sets the path to the template file. The
//!   path is interpreted as relative to the configured template directories
//!   (by default, this is a `templates` directory next to your `Cargo.toml`).
//!   The file name extension is used to infer an escape mode (see below). In
//!   web framework integrations, the path's extension may also be used to
//!   infer the content type of the resulting response.
//!   Cannot be used together with `source`.
//! * `source` (as `source = "{{ foo }}"`): directly sets the template source.
//!   This can be useful for test cases or short templates. The generated path
//!   is undefined, which generally makes it impossible to refer to this
//!   template from other templates. If `source` is specified, `ext` must also
//!   be specified (see below). Cannot be used together with `path`.
//! * `ext` (as `ext = "txt"`): lets you specify the content type as a file
//!   extension. This is used to infer an escape mode (see below), and some
//!   web framework integrations use it to determine the content type.
//!   Cannot be used together with `path`.
//! * `print` (as `print = "code"`): enable debugging by printing nothing
//!   (`none`), the parsed syntax tree (`ast`), the generated code (`code`)
//!   or `all` for both. The requested data will be printed to stdout at
//!   compile time.
//! * `escape` (as `escape = "none"`): override the template's extension used for
//!   the purpose of determining the escaper for this template. See the section
//!   on configuring custom escapers for more information.
//! * `syntax` (as `syntax = "foo"`): set the syntax name for a parser defined
//!   in the configuration file. The default syntax , "default",  is the one
//!   provided by Askama.

#![forbid(unsafe_code)]
#![deny(elided_lifetimes_in_paths)]
#![deny(unreachable_pub)]

mod error;
pub mod filters;
pub mod helpers;

use std::fmt;

pub use askama_derive::Template;
pub use askama_escape::{Html, MarkupDisplay, Text};

#[doc(hidden)]
pub use crate as shared;
pub use crate::error::{Error, Result};

/// Main `Template` trait; implementations are generally derived
///
/// If you need an object-safe template, use [`DynTemplate`].
pub trait Template: fmt::Display {
    /// Helper method which allocates a new `String` and renders into it
    fn render(&self) -> Result<String> {
        let mut buf = String::with_capacity(Self::SIZE_HINT);
        self.render_into(&mut buf)?;
        Ok(buf)
    }

    /// Renders the template to the given `writer` fmt buffer
    fn render_into(&self, writer: &mut (impl std::fmt::Write + ?Sized)) -> Result<()>;

    /// Renders the template to the given `writer` io buffer
    #[inline]
    fn write_into(&self, writer: &mut (impl std::io::Write + ?Sized)) -> std::io::Result<()> {
        writer.write_fmt(format_args!("{}", self))
    }

    /// The template's extension, if provided
    const EXTENSION: Option<&'static str>;

    /// Provides a conservative estimate of the expanded length of the rendered template
    const SIZE_HINT: usize;

    /// The MIME type (Content-Type) of the data that gets rendered by this Template
    const MIME_TYPE: &'static str;
}

/// Object-safe wrapper trait around [`Template`] implementers
///
/// This trades reduced performance (mostly due to writing into `dyn Write`) for object safety.
pub trait DynTemplate {
    /// Helper method which allocates a new `String` and renders into it
    fn dyn_render(&self) -> Result<String>;

    /// Renders the template to the given `writer` fmt buffer
    fn dyn_render_into(&self, writer: &mut dyn std::fmt::Write) -> Result<()>;

    /// Renders the template to the given `writer` io buffer
    fn dyn_write_into(&self, writer: &mut dyn std::io::Write) -> std::io::Result<()>;

    /// Helper function to inspect the template's extension
    fn extension(&self) -> Option<&'static str>;

    /// Provides a conservative estimate of the expanded length of the rendered template
    fn size_hint(&self) -> usize;

    /// The MIME type (Content-Type) of the data that gets rendered by this Template
    fn mime_type(&self) -> &'static str;
}

impl<T: Template> DynTemplate for T {
    fn dyn_render(&self) -> Result<String> {
        <Self as Template>::render(self)
    }

    fn dyn_render_into(&self, writer: &mut dyn std::fmt::Write) -> Result<()> {
        <Self as Template>::render_into(self, writer)
    }

    #[inline]
    fn dyn_write_into(&self, writer: &mut dyn std::io::Write) -> std::io::Result<()> {
        writer.write_fmt(format_args!("{}", self))
    }

    fn extension(&self) -> Option<&'static str> {
        Self::EXTENSION
    }

    fn size_hint(&self) -> usize {
        Self::SIZE_HINT
    }

    fn mime_type(&self) -> &'static str {
        Self::MIME_TYPE
    }
}

impl fmt::Display for dyn DynTemplate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.dyn_render_into(f).map_err(|_| ::std::fmt::Error {})
    }
}

#[cfg(test)]
#[allow(clippy::blacklisted_name)]
mod tests {
    use std::fmt;

    use super::*;
    use crate::{DynTemplate, Template};

    #[test]
    fn dyn_template() {
        struct Test;
        impl Template for Test {
            fn render_into(&self, writer: &mut (impl std::fmt::Write + ?Sized)) -> Result<()> {
                Ok(writer.write_str("test")?)
            }

            const EXTENSION: Option<&'static str> = Some("txt");

            const SIZE_HINT: usize = 4;

            const MIME_TYPE: &'static str = "text/plain; charset=utf-8";
        }

        impl fmt::Display for Test {
            #[inline]
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.render_into(f).map_err(|_| fmt::Error {})
            }
        }

        fn render(t: &dyn DynTemplate) -> String {
            t.dyn_render().unwrap()
        }

        let test = &Test as &dyn DynTemplate;

        assert_eq!(render(test), "test");

        assert_eq!(test.to_string(), "test");

        assert_eq!(format!("{}", test), "test");

        let mut vec = Vec::new();
        test.dyn_write_into(&mut vec).unwrap();
        assert_eq!(vec, vec![b't', b'e', b's', b't']);
    }
}

/// Old build script helper to rebuild crates if contained templates have changed
///
/// This function is now deprecated and does nothing.
#[deprecated(
    since = "0.8.1",
    note = "file-level dependency tracking is handled automatically without build script"
)]
pub fn rerun_if_templates_changed() {}

#[cfg(feature = "localization")]
use fluent_templates::Loader;
#[cfg(feature = "localization")]
pub struct Locale<'a>{
    loader: &'a fluent_templates::StaticLoader,
    language: unic_langid::LanguageIdentifier,
}
#[cfg(feature = "localization")]
impl<'a> Locale<'a> {
    pub fn new(language: unic_langid::LanguageIdentifier, templates:  &'static fluent_templates::StaticLoader) -> Locale<'a> {
        
    Self { 
        loader: templates,
        language,
        }
    }
    pub fn translate(
        &self,
        text_id: &str,
        args:
            &std::collections::HashMap<String, fluent_templates::fluent_bundle::FluentValue<'_>>,
    ) -> String {
        self.loader.lookup_with_args(&self.language, text_id, args)
    }

}
#[macro_export]
macro_rules! init_translation {
    (
        $v: vis $n: ident {
            static_loader_name: $static_loader_name: ident,
            locales: $locales: expr,
            fallback_language: $fallback_language: expr,
            customise: $customise: expr
        }
    ) => {
        use fluent_templates::Loader;
        fluent_templates::static_loader! {
            // Declare our `StaticLoader` named `LOCALES`.
            static $static_loader_name = {
                // The directory of localisations and fluent resources.
                locales: $locales,
                // The language to falback on if something is not present.
                fallback_language: $fallback_language,
                // Optional: A fluent resource that is shared with every locale.
                //core_locales: "/core.ftl",
                // Removes unicode isolating marks around arguments, you typically
                // should only set to false when testing.
                customise: $customise,
            };
        }
        $v struct $n {
            language: unic_langid::LanguageIdentifier,
            loader: &'static fluent_templates::once_cell::sync::Lazy<fluent_templates::StaticLoader>
        }
        impl $n {
            pub fn new(language: unic_langid::LanguageIdentifier) -> $n {
                $n {
                    language,
                    loader: & $static_loader_name
                }
            }
            pub fn defsault() -> $n {
                $n {
                    language: unic_langid::langid!($fallback_language),
                    loader: & $static_loader_name
                }
            }
        }
        impl $n {
            fn get_fallback_language(&self) -> unic_langid::LanguageIdentifier {
                unic_langid::langid!($fallback_language)
            }

            fn get_language(&self) -> unic_langid::LanguageIdentifier {
                self.language.clone()
            }

            fn translate(
                &self,
                text_id: &str,
                args:
                    &std::collections::HashMap<String, fluent_templates::fluent_bundle::FluentValue<'_>>,
            ) -> String {
                self.loader.lookup_with_args(&self.language, text_id, args)
            }

            fn has_default_translation(&self, m: &str) -> bool {
                // lookup_single_language panic's when invalid args are given
                std::panic::set_hook(Box::new(|_info| {
                    // do nothing
                }));

                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    self.loader.lookup_single_language::<String>(&self.get_fallback_language(), m, None)
                }));

                let _ = std::panic::take_hook();

                match result {
                    Ok(None) => false,
                    _ => true
                }
            }
        }

    }
}
