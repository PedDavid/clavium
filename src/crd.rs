//! The `ApiKey` custom resource.
//!
//! `spec` comes from git (applied by GitOps). `status` is written only by
//! this app, through the status subresource.

use k8s_openapi::apimachinery::pkg::apis::meta::v1::{Condition, Time};
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const GROUP: &str = "clavium.prdv.cloud";
pub const VERSION: &str = "v1alpha1";

/// Pattern accepted by [`crate::duration::parse`], e.g. `90d`, `2w`, `12h`.
const DURATION_PATTERN: &str = crate::duration::PATTERN;

#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, Default, PartialEq, JsonSchema)]
#[kube(
    group = "clavium.prdv.cloud",
    version = "v1alpha1",
    kind = "ApiKey",
    plural = "apikeys",
    shortname = "ak",
    namespaced,
    status = "ApiKeyStatus",
    derive = "PartialEq",
    derive = "Default",
    doc = "An external API key: how it is set up, where to renew it and where it is written on rotation.",
    printcolumn = r#"{"name":"Provider","type":"string","jsonPath":".spec.provider"}"#,
    printcolumn = r#"{"name":"Owner","type":"string","jsonPath":".spec.owner"}"#,
    printcolumn = r#"{"name":"Expires","type":"date","jsonPath":".status.expiresAt"}"#,
    printcolumn = r#"{"name":"Rotated","type":"date","jsonPath":".status.lastRotated"}"#,
    printcolumn = r#"{"name":"Valid","type":"string","jsonPath":".status.conditions[?(@.type==\"Valid\")].status"}"#
)]
#[serde(rename_all = "camelCase")]
pub struct ApiKeySpec {
    /// Human-friendly name shown in the UI. Defaults to `metadata.name`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,

    /// Selects the expiry probe run when a new key is submitted.
    #[serde(default)]
    pub provider: Provider,

    /// Person or team responsible for the key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,

    /// Where to renew or recreate the key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub renew_url: Option<String>,

    #[serde(default)]
    pub setup: Setup,

    #[serde(default)]
    pub rotation: RotationPolicy,

    /// Secret stores the key is written to on rotation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<TargetSpec>,

    /// Places that must be updated by hand, shown as a checklist.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub consumers: Vec<String>,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    /// No probe: expiry is entered by hand.
    #[default]
    Generic,
    Github,
    Cloudflare,
}

impl Provider {
    pub fn as_str(self) -> &'static str {
        match self {
            Provider::Generic => "generic",
            Provider::Github => "github",
            Provider::Cloudflare => "cloudflare",
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, Default, PartialEq, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Setup {
    /// Free-text permissions or scopes, shown as badges.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub permissions: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default, PartialEq, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RotationPolicy {
    /// Maximum age before the key must be rotated, e.g. `90d`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(regex(pattern = DURATION_PATTERN))]
    pub max_age: Option<String>,
    /// Warning alert threshold before the deadline (default from config, 14d).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(regex(pattern = DURATION_PATTERN))]
    pub warn_before: Option<String>,
    /// Critical alert threshold before the deadline (default from config, 5d).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(regex(pattern = DURATION_PATTERN))]
    pub critical_before: Option<String>,
}

/// One storage target. `openbao` is the only kind of target and is
/// required, so an empty entry is rejected by the API server.
#[derive(Deserialize, Serialize, Clone, Debug, Default, PartialEq, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TargetSpec {
    pub openbao: OpenBaoTarget,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OpenBaoTarget {
    /// KV v2 mount, e.g. `secret`.
    #[schemars(length(min = 1))]
    pub mount: String,
    /// Path inside the mount, e.g. `ci/renovate`.
    #[schemars(length(min = 1))]
    pub path: String,
    /// Key inside the secret, e.g. `token`.
    #[schemars(length(min = 1))]
    pub key: String,
}

impl OpenBaoTarget {
    /// Stable reference used in status, metrics and logs.
    pub fn reference(&self) -> String {
        format!(
            "openbao/{}/{}#{}",
            self.mount,
            self.path.trim_matches('/'),
            self.key
        )
    }

    /// The API path as used in OpenBao policies: `<mount>/data/<path>`.
    pub fn policy_path(&self) -> String {
        format!(
            "{}/data/{}",
            self.mount.trim_matches('/'),
            self.path.trim_matches('/')
        )
    }
}

impl TargetSpec {
    pub fn reference(&self) -> String {
        self.openbao.reference()
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, Default, PartialEq, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApiKeyStatus {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_rotated: Option<Time>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotated_by: Option<String>,
    /// Effective expiry: the probed value if there is one, otherwise the manual one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<Time>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at_source: Option<ExpirySource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manual_expires_at: Option<Time>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub probe: Option<ProbeStatus>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<TargetStatus>,
    /// Most recent first, bounded to [`HISTORY_LIMIT`] entries.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<HistoryEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<Condition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_generation: Option<i64>,
}

pub const HISTORY_LIMIT: usize = 10;

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ExpirySource {
    Probe,
    Manual,
    None,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProbeStatus {
    pub at: Time,
    /// Who the key belongs to according to the provider. Never the key itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<Time>,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TargetStatus {
    #[serde(rename = "ref")]
    pub reference: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_written: Option<Time>,
    pub result: TargetResult,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum TargetResult {
    Ok,
    Failed,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub at: Time,
    pub by: String,
    pub kind: HistoryKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<Time>,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum HistoryKind {
    /// A new key was submitted and written to the targets.
    Rotated,
    /// Dates were recorded by hand, without a key.
    Recorded,
}

impl ApiKey {
    pub fn display_name(&self) -> &str {
        self.spec
            .display_name
            .as_deref()
            .unwrap_or_else(|| self.metadata.name.as_deref().unwrap_or_default())
    }

    pub fn name(&self) -> &str {
        self.metadata.name.as_deref().unwrap_or_default()
    }

    pub fn namespace(&self) -> &str {
        self.metadata.namespace.as_deref().unwrap_or_default()
    }

    pub fn condition(&self, type_: &str) -> Option<&Condition> {
        self.status
            .as_ref()?
            .conditions
            .iter()
            .find(|c| c.type_ == type_)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_example() {
        let yaml = r#"
apiVersion: clavium.prdv.cloud/v1alpha1
kind: ApiKey
metadata: { name: renovate-github, namespace: clavium }
spec:
  provider: github
  renewUrl: https://github.com/settings/personal-access-tokens
  setup: { permissions: ["contents: read"] }
  rotation: { maxAge: 90d }
  targets:
    - openbao: { mount: secret, path: ci/renovate, key: token }
"#;
        let key: ApiKey = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(key.spec.provider, Provider::Github);
        assert_eq!(key.display_name(), "renovate-github");
        let target = &key.spec.targets[0].openbao;
        assert_eq!(target.reference(), "openbao/secret/ci/renovate#token");
        assert_eq!(target.policy_path(), "secret/data/ci/renovate");
    }

    #[test]
    fn defaults_are_minimal() {
        let key: ApiKey = serde_yaml::from_str(
            "apiVersion: clavium.prdv.cloud/v1alpha1\nkind: ApiKey\nmetadata: {name: a}\nspec: {}\n",
        )
        .unwrap();
        assert_eq!(key.spec.provider, Provider::Generic);
        assert!(key.spec.targets.is_empty());
    }

    #[test]
    fn empty_target_is_rejected() {
        let err = serde_yaml::from_str::<ApiKeySpec>("targets: [{}]\n").unwrap_err();
        assert!(err.to_string().contains("missing field `openbao`"), "{err}");
    }

    /// The generated schema, as the API server uses it for admission.
    fn spec_schema() -> serde_json::Value {
        use kube::CustomResourceExt;
        let crd = serde_json::to_value(ApiKey::crd()).unwrap();
        crd["spec"]["versions"][0]["schema"]["openAPIV3Schema"]["properties"]["spec"].clone()
    }

    #[test]
    fn schema_requires_a_target_kind() {
        let target = &spec_schema()["properties"]["targets"]["items"];
        assert_eq!(target["required"], serde_json::json!(["openbao"]));
        let openbao = &target["properties"]["openbao"];
        assert_eq!(
            openbao["required"],
            serde_json::json!(["key", "mount", "path"])
        );
    }

    #[test]
    fn schema_durations_use_the_parser_pattern() {
        let rotation = &spec_schema()["properties"]["rotation"]["properties"];
        for field in ["maxAge", "warnBefore", "criticalBefore"] {
            assert_eq!(
                rotation[field]["pattern"],
                crate::duration::PATTERN,
                "{field}"
            );
        }
    }
}
