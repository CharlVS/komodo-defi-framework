use async_trait::async_trait;
use common::executor::AbortedError;
use derive_more::Display;
use futures::lock::Mutex as AsyncMutex;
use std::fmt::Debug;
use std::ops::Deref;
use std::sync::Arc;

use super::ElectrumConnection;

/// Trait provides a common interface to get an `ElectrumConnection` from the `ElectrumClient` instance
#[async_trait]
pub(super) trait ConnectionManagerTrait: Debug {
    async fn get_conn(&self) -> Vec<Arc<AsyncMutex<ElectrumConnection>>>;
    async fn get_conn_by_address(
        &self,
        address: &str,
    ) -> Result<Arc<AsyncMutex<ElectrumConnection>>, ConnectionManagerErr>;
    async fn connect(&self) -> Result<(), ConnectionManagerErr>;
    async fn is_connected(&self) -> bool;
    async fn remove_server(&self, address: &str) -> Result<(), ConnectionManagerErr>;
    async fn rotate_servers(&self, no_of_rotations: usize);
    async fn is_connections_pool_empty(&self) -> bool;
    fn on_disconnected(&self, address: &str);
}

#[async_trait]
impl ConnectionManagerTrait for Arc<dyn ConnectionManagerTrait + Send + Sync> {
    async fn get_conn(&self) -> Vec<Arc<AsyncMutex<ElectrumConnection>>> { self.deref().get_conn().await }
    async fn get_conn_by_address(
        &self,
        address: &str,
    ) -> Result<Arc<AsyncMutex<ElectrumConnection>>, ConnectionManagerErr> {
        self.deref().get_conn_by_address(address).await
    }
    async fn connect(&self) -> Result<(), ConnectionManagerErr> { self.deref().connect().await }
    async fn is_connected(&self) -> bool { self.deref().is_connected().await }
    async fn remove_server(&self, address: &str) -> Result<(), ConnectionManagerErr> {
        self.deref().remove_server(address).await
    }
    async fn rotate_servers(&self, no_of_rotations: usize) { self.deref().rotate_servers(no_of_rotations).await }
    async fn is_connections_pool_empty(&self) -> bool { self.deref().is_connections_pool_empty().await }
    fn on_disconnected(&self, address: &str) { self.deref().on_disconnected(address) }
}

#[derive(Debug, Display)]
pub(super) enum ConnectionManagerErr {
    #[display(fmt = "Unknown address: {}", _0)]
    UnknownAddress(String),
    #[display(fmt = "Connection is not established, {}", _0)]
    NotConnected(String),
    #[display(fmt = "Failed to abort abortable system for: {}, error: {}", _0, _1)]
    FailedAbort(String, AbortedError),
    #[display(fmt = "Failed to connect to: {}, error: {}", _0, _1)]
    ConnectingError(String, String),
    #[display(fmt = "No settings to connect to found")]
    SettingsNotSet,
}

/// This timeout implies both connecting and verifying phases time
pub const DEFAULT_CONN_TIMEOUT_SEC: u64 = 20;
pub const SUSPEND_TIMEOUT_INIT_SEC: u64 = 30;
