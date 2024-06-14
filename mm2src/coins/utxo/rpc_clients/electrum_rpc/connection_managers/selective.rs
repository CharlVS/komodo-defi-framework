use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock, Weak};

use super::super::client::{ElectrumClientImpl};
use super::super::connection::{ElectrumConnection, ElectrumConnectionSettings};
use super::super::constants::PING_INTERVAL;
use super::connection_context::ConnectionContext;
use super::{ConnectionManagerTrait};

use crate::utxo::rpc_clients::UtxoRpcClientOps;
use common::executor::abortable_queue::AbortableQueue;
use common::executor::abortable_queue::WeakSpawner;
use common::executor::{AbortableSystem, SpawnFuture, Timer};
use common::log::warn;
use futures::channel::mpsc;
use futures::{StreamExt};
use keys::Address;

use async_trait::async_trait;
use futures::compat::Future01CompatExt;
use gstuff::now_ms;

#[derive(Debug)]
pub struct ConnectionManagerSelective {
    /// A flag to spawn a ping loop task for the active connection.
    spawn_ping: bool,
    /// The address currently active connection.
    active_address: Mutex<Option<String>>,
    /// A channel to notify the background task that some active connection has disconnected.
    active_address_disconnection_notifier: mpsc::Sender<()>,
    /// A receiver to be used by the background task to wait for disconnection notifications.
    /// We wrap it around a mutex to be able to take it out when the background task starts.
    active_address_disconnection_receiver: Mutex<Option<mpsc::Receiver<()>>>,
    /// A map for server addresses to their corresponding connections.
    connections: HashMap<String, ConnectionContext>,
    /// A weak reference to the electrum client that owns this connection manager.
    /// It is used to send electrum requests during connection establishment (version querying).
    // TODO: This field might not be necessary if [`ElectrumConnection`] object be used to send
    // electrum requests on its own, i.e. implement [`JsonRpcClient`] & [`UtxoRpcClientOps`].
    electrum_client: RwLock<Option<Weak<ElectrumClientImpl>>>,
}

impl ConnectionManagerSelective {
    pub fn try_new_arc(
        servers: Vec<ElectrumConnectionSettings>,
        spawn_ping: bool,
        abortable_system: &AbortableQueue,
    ) -> Result<Arc<Self>, String> {
        let mut connections = HashMap::with_capacity(servers.len());
        for connection_settings in servers {
            let subsystem = abortable_system.create_subsystem().map_err(|e| {
                ERRL!(
                    "Failed to create abortable subsystem for connection: {}, error: {:?}",
                    connection_settings.url,
                    e
                )
            })?;

            let connection = ElectrumConnection::new(connection_settings, subsystem);
            connections.insert(connection.address().to_string(), ConnectionContext::new(connection));
        }

        // Create a channel to be shared between the connection manager and its background task
        // to notify the background task that some active connection has disconnected.
        let (sender, receiver) = mpsc::channel::<()>(0);

        Ok(Arc::new(ConnectionManagerSelective {
            spawn_ping,
            active_address: Mutex::new(None),
            connections,
            electrum_client: RwLock::new(None),
            active_address_disconnection_notifier: sender,
            active_address_disconnection_receiver: Mutex::new(Some(receiver)),
        }))
    }

    fn get_primary_connections(&self) -> impl Iterator<Item = Arc<ElectrumConnection>> + '_ {
        self.connections.values().filter_map(|connection_ctx| {
            connection_ctx
                .connection
                .is_primary()
                .then_some(connection_ctx.connection.clone())
        })
    }

    fn get_secondary_connections(&self) -> impl Iterator<Item = Arc<ElectrumConnection>> + '_ {
        self.connections.values().filter_map(|connection_ctx| {
            connection_ctx
                .connection
                .is_secondary()
                .then_some(connection_ctx.connection.clone())
        })
    }

    /// Triggers the background task to reconnect to a server.
    fn trigger_reconnection(&self) {
        // Notify the background task that some active connection has disconnected.
        // Note that `try_send` might fail if the channel is out of capacity
        // (the manager already has an unpolled notification) or if the receiver
        // is dropped (which won't happen unless the background task is stopped).
        self.active_address_disconnection_notifier.clone().try_send(()).ok();
    }
}

// FIXME: A lot of the methods here are c/v from the multiple connection manager.
// Generalize by having them in the trait default implementation instead.
#[async_trait]
impl ConnectionManagerTrait for Arc<ConnectionManagerSelective> {
    fn copy(&self) -> Box<dyn ConnectionManagerTrait> { Box::new(self.clone()) }

    fn spawn_ping(&self) -> bool { self.spawn_ping }

    fn connections(&self) -> &HashMap<String, ConnectionContext> { &self.connections }

    fn weak_client(&self) -> &RwLock<Option<Weak<ElectrumClientImpl>>> { &self.electrum_client }

    async fn get_active_connections(&self) -> Vec<Arc<ElectrumConnection>> {
        // We might have some other connected servers (e.g. was connected because of connection by address),
        // but we will only return the one we know it's active right now. This also avoids sending pings to
        // connections that are not used thus keeping their connections alive.
        let maybe_active_connection = self
            .active_address
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|address| self.get_connection(address));
        if let Some(active_connection) = maybe_active_connection {
            if active_connection.is_connected().await {
                // There is only one active connection at a time.
                return vec![active_connection];
            }
        }
        // We don't currently have an active connection.
        self.trigger_reconnection();
        vec![]
    }

    /// Subscribe the list of addresses to our active connections.
    ///
    /// There is a bit of indirection here. We register the abandoned addresses on `on_disconnected` with
    /// the client to queue them for `utxo_balance_events` which in turn calls this method back to re-subscribe
    /// the abandoned addresses. We could have instead directly re-subscribed the addresses here in the connection
    /// manager without sending them to `utxo_balance_events`. However, we don't do that so that `utxo_balance_events`
    /// knows about all the added addresses. If it doesn't know about them, it won't be able to retrieve the triggered
    /// address when its script hash is notified.
    async fn add_subscriptions(&self, addresses: &HashMap<String, Address>) {
        // FIXME: The multiple connection manager implementation of this method is still
        // compatible here. Let's just use both as trait default method.
        for (scripthash, address) in addresses.iter() {
            // Keep trying to subscribe the address until successful.
            loop {
                let Some(client) = self.get_client() else {
                    // The manager is either not initialized or the client is dropped.
                    // If the client is dropped, the manager is no longer usable.
                    return;
                };
                let active_connection = self.get_active_connections().await.first().cloned();
                let Some(active_connection) = active_connection else {
                    // If there is no active connection, wait for a connection to be established/activated.
                    Timer::sleep(1.).await;
                    continue;
                };
                if client
                    .blockchain_scripthash_subscribe_using(active_connection.address(), scripthash.clone())
                    .compat()
                    .await
                    .is_ok()
                {
                    if let Some(connection_ctx) = self.connections.get(active_connection.address()) {
                        connection_ctx.add_sub(address.clone());
                        // Address subscribed, move to the next one.
                        break;
                    }
                }
            }
        }
    }

    fn on_disconnected(&self, server_address: &str) {
        // If the disconnected connection is the active connection, mark that.
        let mut active_connection = self.active_address.lock().unwrap();
        if active_connection.as_deref() == Some(server_address) {
            *active_connection = None;
            self.trigger_reconnection();
        }
        drop(active_connection);

        let Some(connection_ctx) = self.connections.get(server_address) else {
            warn!("No connection found for address: {server_address}");
            return;
        };
        let abandoned_subs = connection_ctx.disconnected();
        // Re-subscribe the abandoned addresses using the client.
        if let Some(client) = self.get_client() {
            client.subscribe_addresses(abandoned_subs.into_iter().collect()).ok();
        };
    }

    fn spawn_ping_task(&self, spawner: &WeakSpawner) {
        let manager = self.clone();
        let task = async move {
            loop {
                let Some(client) = manager.get_client() else { break };
                // This will ping all the active connections, which will keep these connections alive.
                client.server_ping().compat().await.ok();
                Timer::sleep(PING_INTERVAL).await;
            }
        };
        spawner.spawn(task);
    }

    fn spawn_background_task(&self, spawner: &WeakSpawner) {
        let manager = self.clone();
        let task = async move {
            // Take out the notifier (receiver) from the connection manager.
            let Some(mut disconnection_notification) = manager.active_address_disconnection_receiver.lock().unwrap().take() else { return };
            loop {
                let Some(client) = manager.get_client() else { return };
                // We are disconnected at the point, try to connect to a server.
                // List all the primary connections first, then the secondary ones.
                let mut connections = manager.get_primary_connections().collect::<Vec<_>>();
                connections.extend(manager.get_secondary_connections());

                for connection in connections {
                    let Some(connection_ctx) = manager.connections.get(connection.address()) else { continue };
                    // Try to connect to the server if it's not suspended.
                    if now_ms() >= connection_ctx.suspend_until() {
                        let address = connection.address().to_string();
                        if ElectrumConnection::establish_connection_loop(connection, client.clone())
                            .await
                            .is_ok()
                        {
                            // We are connected, mark the connection as active.
                            *manager.active_address.lock().unwrap() = Some(address);
                            break;
                        }
                    }
                }

                if !manager.get_active_connections().await.is_empty() {
                    // Since we are connected, wait for a disconnection notification
                    // before we try connecting to another server.
                    disconnection_notification.next().await;
                }
            }
        };
        spawner.spawn(task);
    }
}
