//! Package publishing for the n3v3 registry.
//! n3v3 注册表的软件包发布。

use crate::output;
use std::path::PathBuf;

pub fn run(package_dir: &str, registry_url: Option<&str>) -> Result<(), String> {
    let url = registry_url
        .map(|s| s.to_string())
        .or_else(|| std::env::var("N3V3_REGISTRY").ok())
        .ok_or_else(|| "N3V3_REGISTRY not set and no --registry-url provided".to_string())?;
    let token = super::registry_serve::read_registry_token()?;

    let dir = PathBuf::from(package_dir);
    if !dir.exists() {
        return Err(format!("directory '{}' not found", package_dir));
    }

    // Look for flake.n3v3 or package.n3v3
    let flake_path = dir.join("flake.n3v3");
    let package_path = dir.join("package.n3v3");

    let manifest_path = if flake_path.exists() {
        flake_path
    } else if package_path.exists() {
        package_path
    } else {
        return Err("no flake.n3v3 or package.n3v3 found in directory".to_string());
    };

    let manifest = std::fs::read_to_string(&manifest_path)
        .map_err(|e| format!("failed to read manifest: {e}"))?;

    // Extract package name and version from manifest
    let name = extract_field(&manifest, "name")?;
    let version = extract_field(&manifest, "version")?;
    let description = extract_field(&manifest, "description").unwrap_or_default();

    output::info(&format!("Publishing {name} v{version}"));

    // Build package metadata
    let metadata = serde_json::json!({
        "name": name,
        "version": version,
        "description": description,
        "manifest": manifest,
    });

    let body = serde_json::to_string(&metadata)
        .map_err(|e| format!("failed to serialize metadata: {e}"))?;

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("failed to create HTTP client: {e}"))?;

    let publish_url = format!("{}/v1/packages/{name}", url.trim_end_matches('/'));
    let response = client
        .post(&publish_url)
        .bearer_auth(token)
        .header("Content-Type", "application/json")
        .body(body)
        .send()
        .map_err(|e| format!("failed to publish: {e}"))?;

    if response.status().is_success() {
        output::info(&format!("Successfully published {name} v{version}"));
        Ok(())
    } else {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        Err(format!("publish failed: HTTP {status}: {body}"))
    }
}

fn extract_field(manifest: &str, field: &str) -> Result<String, String> {
    for line in manifest.lines() {
        let Some((key, raw_value)) = line.trim().split_once('=') else {
            continue;
        };
        if key.trim() != field {
            continue;
        }
        let raw_value = raw_value.trim();
        let scalar = raw_value
            .strip_suffix(',')
            .or_else(|| raw_value.strip_suffix(';'))
            .unwrap_or(raw_value)
            .trim();
        let invalid = || {
            format!(
                "field '{field}' in manifest must be a non-empty scalar with matched double quotes"
            )
        };
        let value = if scalar.starts_with('"') || scalar.ends_with('"') {
            let inner = scalar
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'))
                .ok_or_else(invalid)?;
            if inner.is_empty() || has_invalid_quotes(inner) {
                return Err(invalid());
            }
            inner
        } else if scalar.is_empty() || scalar.contains('"') {
            return Err(invalid());
        } else {
            scalar
        };
        return Ok(value.to_string());
    }
    Err(format!("field '{field}' not found in manifest"))
}

fn has_invalid_quotes(value: &str) -> bool {
    let mut is_escaped = false;
    for character in value.chars() {
        if is_escaped {
            is_escaped = false;
        } else if character == '\\' {
            is_escaped = true;
        } else if character == '"' {
            return true;
        }
    }
    is_escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_field_quoted_value_with_semicolon_returns_unquoted_value() {
        let manifest = "name = \"example\";\nversion = \"1.0.0\";\n";
        assert_eq!(extract_field(manifest, "name").unwrap(), "example");
        assert_eq!(extract_field(manifest, "version").unwrap(), "1.0.0");
    }

    #[test]
    fn extract_field_init_flake_extracts_name_and_version() {
        let dir = tempfile::tempdir().expect("temporary directory");
        let project = dir.path().join("demo");
        super::super::init::run(project.to_str().expect("UTF-8 temporary path"))
            .expect("init should write flake.n3v3");
        let manifest = std::fs::read_to_string(project.join("flake.n3v3"))
            .expect("generated manifest should be readable");
        assert_eq!(extract_field(&manifest, "name").unwrap(), "demo");
        assert_eq!(extract_field(&manifest, "version").unwrap(), "0.1.0");
    }

    #[test]
    fn extract_field_malformed_quoted_value_returns_user_error() {
        for value in ["\"demo", "demo\"", "\"\"", "\"de\"mo\"", "\"demo\",,", ""] {
            let manifest = format!("name = {value},");
            let error = extract_field(&manifest, "name").unwrap_err();
            assert!(error.contains("field 'name' in manifest"), "{error}");
            assert!(error.contains("matched double quotes"), "{error}");
        }
    }
}
