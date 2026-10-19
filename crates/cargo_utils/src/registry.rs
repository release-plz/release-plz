pub fn cargo_registries_token_env_var_name(registry: &str) -> anyhow::Result<String> {
    let name = if registry == "crates-io" {
        "CARGO_REGISTRY_TOKEN".to_string()
    } else {
        format!(
            "CARGO_REGISTRIES_{}_TOKEN",
            registry_env_var_name(registry)?
        )
    };
    Ok(name)
}

/// Sanitizes the registry name to construct a valid environment variable name.
/// Mirrors Cargo's behavior:
/// - Alphanumeric characters and underscores (`_`) are preserved.
/// - Hyphens (`-`) are converted to underscores (`_`).
/// - Any other non-alphanumeric character is invalid and will cause an error.
fn registry_env_var_name(registry: &str) -> anyhow::Result<String> {
    let mut sanitized_name = String::with_capacity(registry.len());

    for ch in registry.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            sanitized_name.push(ch);
        } else if ch == '-' {
            sanitized_name.push('_');
        } else {
            anyhow::bail!("Invalid character in registry name `{registry}`: `{ch}`");
        }
    }

    Ok(sanitized_name.to_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_registry_env_var_name() {
        assert_eq!(registry_env_var_name("my-registry").unwrap(), "MY_REGISTRY");
        assert_eq!(registry_env_var_name("my_registry").unwrap(), "MY_REGISTRY");
        assert_eq!(registry_env_var_name("registry1").unwrap(), "REGISTRY1");
        assert_eq!(registry_env_var_name("UPPERCASE").unwrap(), "UPPERCASE");
        assert_eq!(
            registry_env_var_name("-leading-dash").unwrap(),
            "_LEADING_DASH"
        );
        assert_eq!(
            registry_env_var_name("trailing-dash-").unwrap(),
            "TRAILING_DASH_"
        );
        assert_eq!(
            registry_env_var_name("multiple---dashes").unwrap(),
            "MULTIPLE___DASHES"
        );
        assert_eq!(
            registry_env_var_name("mixed-dashes-and_underscores").unwrap(),
            "MIXED_DASHES_AND_UNDERSCORES"
        );
        assert_eq!(registry_env_var_name("---").unwrap(), "___");

        // Invalid characters should fail

        expect_test::expect!["Invalid character in registry name `with-special-chars!`: `!`"]
            .assert_eq(&registry_env_var_name_error("with-special-chars!"));

        expect_test::expect!["Invalid character in registry name `invalid+char`: `+`"]
            .assert_eq(&registry_env_var_name_error("invalid+char"));

        expect_test::expect!["Invalid character in registry name `has@symbol`: `@`"]
            .assert_eq(&registry_env_var_name_error("has@symbol"));

        expect_test::expect!["Invalid character in registry name `space not allowed`: ` `"]
            .assert_eq(&registry_env_var_name_error("space not allowed"));
    }

    #[test]
    fn cargo_info_token_env_var_name_for_crates_io_works() {
        assert_eq!(
            cargo_registries_token_env_var_name("crates-io").unwrap(),
            "CARGO_REGISTRY_TOKEN"
        );
    }

    #[test]
    fn cargo_info_token_env_var_name_normalizes_dashes() {
        assert_eq!(
            cargo_registries_token_env_var_name("my-registry").unwrap(),
            "CARGO_REGISTRIES_MY_REGISTRY_TOKEN"
        );
    }

    #[test]
    fn cargo_info_token_env_var_name_invalid_registry_fails() {
        let error = cargo_registries_token_env_var_name("invalid+registry")
            .unwrap_err()
            .to_string();
        assert_eq!(
            error,
            "Invalid character in registry name `invalid+registry`: `+`"
        );
    }

    fn registry_env_var_name_error(registry: &str) -> String {
        registry_env_var_name(registry).unwrap_err().to_string()
    }
}
