//! Fluent localization bootstrap. User-facing keys live in `locales/` and
//! callers can add locale bundles without changing protocol or security code.

use fluent_bundle::{FluentBundle, FluentResource};
use unic_langid::LanguageIdentifier;

pub type Bundle = FluentBundle<FluentResource>;

/// Load the canonical English bundle embedded in the application binary.
pub fn english_bundle() -> Result<Bundle, String> {
    let locale: LanguageIdentifier = "en-US"
        .parse()
        .map_err(|err| format!("invalid locale: {err}"))?;
    let resource = FluentResource::try_new(include_str!("../../locales/en-US/app.ftl").to_owned())
        .map_err(|(_, errors)| format!("invalid English Fluent resource: {errors:?}"))?;
    let mut bundle = FluentBundle::new(vec![locale]);
    bundle
        .add_resource(resource)
        .map_err(|errors| format!("could not load Fluent resource: {errors:?}"))?;
    Ok(bundle)
}

#[cfg(test)]
mod tests {
    #[test]
    fn english_resource_loads() {
        let bundle = super::english_bundle().unwrap();
        assert!(bundle.has_message("app-name"));
    }
}
