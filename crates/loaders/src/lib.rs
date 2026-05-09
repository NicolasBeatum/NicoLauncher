pub mod fabric;
pub mod merge;

pub use fabric::FabricProvider;
pub use merge::{LoaderProfile, merge};

use async_trait::async_trait;
use launcher_core::{LoaderType, Result};

#[async_trait]
pub trait LoaderProvider: Send + Sync {
    fn id(&self) -> LoaderType;
    async fn list_versions(&self, mc_version: &str) -> Result<Vec<String>>;
    async fn recommended_version(&self, mc_version: &str) -> Result<String>;
}
