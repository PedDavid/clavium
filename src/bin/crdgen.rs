//! Prints the `ApiKey` CustomResourceDefinition as YAML.

use clavium::crd::ApiKey;
use kube::CustomResourceExt;

fn main() -> anyhow::Result<()> {
    print!("{}", serde_yaml::to_string(&ApiKey::crd())?);
    Ok(())
}
