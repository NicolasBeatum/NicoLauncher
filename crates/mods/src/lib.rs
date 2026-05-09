//! Mod providers: Modrinth, CurseForge, SelfHosted.
//! Implemented in Phase 3.

use async_trait::async_trait;
use launcher_core::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ModSource {
    Modrinth {
        project_id: String,
        version_id: String,
        download_url: Option<String>,
    },
    CurseForge {
        project_id: u64,
        file_id: u64,
        download_url: Option<String>,
    },
    SelfHosted {
        url: String,
    },
}

#[derive(Debug)]
pub struct ResolvedMod {
    pub download_url: String,
    pub sha512: String,
    pub size: u64,
    pub filename: String,
}

#[async_trait]
pub trait ModProvider: Send + Sync {
    async fn resolve(&self, source: &ModSource) -> Result<ResolvedMod>;
    fn name(&self) -> &str;
}
