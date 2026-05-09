use std::fmt;
use std::str::FromStr;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LoaderType {
    Vanilla,
    Fabric,
    Quilt,
    NeoForge,
    Forge,
}

impl fmt::Display for LoaderType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoaderType::Vanilla  => write!(f, "vanilla"),
            LoaderType::Fabric   => write!(f, "fabric"),
            LoaderType::Quilt    => write!(f, "quilt"),
            LoaderType::NeoForge => write!(f, "neoforge"),
            LoaderType::Forge    => write!(f, "forge"),
        }
    }
}

impl FromStr for LoaderType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "vanilla"  => Ok(LoaderType::Vanilla),
            "fabric"   => Ok(LoaderType::Fabric),
            "quilt"    => Ok(LoaderType::Quilt),
            "neoforge" => Ok(LoaderType::NeoForge),
            "forge"    => Ok(LoaderType::Forge),
            other      => Err(format!("Unknown loader type: {other}")),
        }
    }
}
