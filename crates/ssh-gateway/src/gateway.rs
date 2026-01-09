use russh::server::*;
use russh::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};

#[derive(Debug, Clone)]
pub struct BackendConfig {
    pub login_pass: Option<String>,
    pub addr: String,
    pub user: String,
    pub pass: String,
}

pub struct BackendRegistry(pub Arc<RwLock<HashMap<String, BackendConfig>>>);

pub struct Gateway {
    backends: BackendRegistry,
}

impl Gateway {
    pub fn new() -> Self {
        Self {
            backends: BackendRegistry(Arc::new(RwLock::new(HashMap::new()))),
        }
    }

    pub fn backend_registry(&self) -> BackendRegistry {
        BackendRegistry(Arc::clone(&self.backends.0))
    }
}

impl BackendRegistry {
    pub async fn add_backend(&self, user: String, config: BackendConfig) {
        let mut backends = self.0.write().await;
        backends.insert(user, config);
    }

    pub async fn remove_backend(&self, user: &str) {
        let mut backends = self.0.write().await;
        backends.remove(user);
    }
}

impl Server for Gateway {
    type Handler = GatewayHandler;

    fn new_client(&mut self, _peer_addr: Option<std::net::SocketAddr>) -> Self::Handler {
        GatewayHandler {
            backends: Arc::clone(&self.backends.0),
            authenticated_user: None,
            authenticated_pass: None,
            selected_backend: None,
            pty_info: None,
            env_vars: HashMap::new(),
            backend_session: None,
            client_to_backend_tx: None,
        }
    }
}

pub struct GatewayHandler {
    backends: Arc<RwLock<HashMap<String, BackendConfig>>>,
    authenticated_user: Option<String>,
    authenticated_pass: Option<String>,
    selected_backend: Option<BackendConfig>,
    pty_info: Option<(String, u32, u32, u32, u32, Vec<(Pty, u32)>)>,
    env_vars: HashMap<String, String>,
    backend_session: Option<russh::client::Handle<ClientHandler>>,
    client_to_backend_tx: Option<mpsc::UnboundedSender<Vec<u8>>>,
}

impl Handler for GatewayHandler {
    type Error = anyhow::Error;

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        tracing::info!("Client authenticating as user: {}", user);

        self.authenticated_user = Some(user.to_string());
        self.authenticated_pass = Some(password.to_string());

        let backends = self.backends.read().await;
        if let Some(backend) = backends.get(user) {
            if backend
                .login_pass
                .as_ref()
                .is_some_and(|pass| pass != password)
            {
                tracing::warn!("Authentication failed for user: {}", user);
                return Ok(Auth::Reject {
                    partial_success: false,
                    proceed_with_methods: None,
                });
            }
            self.selected_backend = Some(backend.clone());
            tracing::info!("Matched backend for user: {}", user);
        } else {
            tracing::warn!("No backend found for user: {}", user);
                return Ok(Auth::Reject {
                    partial_success: false,
                    proceed_with_methods: None,
                });
        }

        Ok(Auth::Accept)
    }

    async fn channel_open_session(
        &mut self,
        _channel: russh::Channel<Msg>,
        _session: &mut Session,
    ) -> Result<bool, Self::Error> {
        tracing::debug!("Channel open session request");
        Ok(true)
    }

    async fn authentication_banner(&mut self) -> Result<Option<String>, Self::Error> {
        Ok(Some("Plfanzen SSH Gateway - Connecting you to your backend server.\n\nPlease note: Certain SSH features, like remote port forwarding, are not supported and may lead to connection issues.\nPlease wait 3 seconds for the connection to proceed.\n\n".to_string()))
    }

    async fn pty_request(
        &mut self,
        _channel: ChannelId,
        term: &str,
        col_width: u32,
        row_height: u32,
        pix_width: u32,
        pix_height: u32,
        modes: &[(Pty, u32)],
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        tracing::debug!(
            "PTY request: term={}, size={}x{}",
            term,
            col_width,
            row_height
        );
        self.pty_info = Some((
            term.to_string(),
            col_width,
            row_height,
            pix_width,
            pix_height,
            modes.to_vec(),
        ));
        Ok(())
    }

    async fn env_request(
        &mut self,
        _channel: ChannelId,
        name: &str,
        value: &str,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        tracing::debug!("Environment variable: {}={}", name, value);
        self.env_vars.insert(name.to_string(), value.to_string());
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        tracing::info!("Shell request - connecting to backend");
        self.start_backend_session(channel, session, None).await
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let command = String::from_utf8_lossy(data).to_string();
        tracing::info!("Exec request: {}", command);
        self.start_backend_session(channel, session, Some(command))
            .await
    }

    async fn data(
        &mut self,
        _channel: ChannelId,
        data: &[u8],
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        if let Some(tx) = &self.client_to_backend_tx {
            let _ = tx.send(data.to_vec());
        }
        Ok(())
    }

    async fn channel_eof(
        &mut self,
        _channel: ChannelId,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        tracing::debug!("Client sent EOF");
        self.client_to_backend_tx = None;
        Ok(())
    }

    async fn channel_open_direct_tcpip(
        &mut self,
        channel: Channel<Msg>,
        host_to_connect: &str,
        port_to_connect: u32,
        originator_address: &str,
        originator_port: u32,
        session: &mut Session,
    ) -> Result<bool, Self::Error> {
        tracing::info!(
            "Direct TCP/IP request: {}:{} (from {}:{})",
            host_to_connect,
            port_to_connect,
            originator_address,
            originator_port
        );

        // Get backend configuration
        let backend = self
            .selected_backend
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No backend selected"))?;

        // Connect to backend and open forwarding channel
        let config = russh::client::Config::default();
        let config = Arc::new(config);

        let mut backend_session =
            russh::client::connect(config, &backend.addr, ClientHandler).await?;
        let auth_res = backend_session
            .authenticate_password(&backend.user, &backend.pass)
            .await?;

        if !matches!(auth_res, russh::client::AuthResult::Success) {
            return Ok(false);
        }

        // Request direct-tcpip channel from backend
        let mut backend_channel = backend_session
            .channel_open_direct_tcpip(
                host_to_connect,
                port_to_connect,
                originator_address,
                originator_port,
            )
            .await?;

        // Create mpsc channel for forwarding
        let (tx, mut rx) = mpsc::unbounded_channel();
        self.client_to_backend_tx = Some(tx);
        self.backend_session = Some(backend_session);

        // Spawn bidirectional forwarding task
        let handle = session.handle();
        let channel_id = channel.id();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    Some(data) = rx.recv() => {
                        if let Err(e) = backend_channel.data(&data[..]).await {
                            tracing::error!("Failed to forward to backend: {:?}", e);
                            break;
                        }
                    }
                    msg = backend_channel.wait() => {
                        match msg {
                            Some(russh::ChannelMsg::Data { data }) => {
                                if let Err(e) = handle.data(channel_id, data.into()).await {
                                    tracing::error!("Failed to forward to client: {:?}", e);
                                    break;
                                }
                            }
                            Some(russh::ChannelMsg::Eof) | Some(russh::ChannelMsg::Close) | None => {
                                let _ = handle.close(channel_id).await;
                                break;
                            }
                            _ => {}
                        }
                    }
                }
            }
        });

        Ok(true)
    }

    async fn tcpip_forward(
        &mut self,
        address: &str,
        port: &mut u32,
        _session: &mut Session,
    ) -> Result<bool, Self::Error> {
        tracing::warn!(
            "Remote TCP/IP forward request rejected: {}:{} - Remote forwarding not supported",
            address,
            port
        );
        Ok(false)
    }

    async fn cancel_tcpip_forward(
        &mut self,
        address: &str,
        port: u32,
        _session: &mut Session,
    ) -> Result<bool, Self::Error> {
        tracing::debug!("Cancel TCP/IP forward ignored: {}:{}", address, port);
        // Nothing to cancel since remote forwarding is not supported
        Ok(false)
    }
}

impl GatewayHandler {
    async fn start_backend_session(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
        command: Option<String>,
    ) -> anyhow::Result<()> {
        let backend = self
            .selected_backend
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No backend selected"))?;

        let backend_user = &backend.user;
        let backend_pass = &backend.pass;

        tracing::info!("Connecting to backend: {}", backend.addr);

        let config = russh::client::Config::default();
        let config = Arc::new(config);

        let mut backend_session =
            russh::client::connect(config, &backend.addr, ClientHandler).await?;

        let auth_res = backend_session
            .authenticate_password(backend_user, backend_pass)
            .await?;

        if !matches!(auth_res, russh::client::AuthResult::Success) {
            anyhow::bail!("Backend authentication failed");
        }

        tracing::info!("Successfully authenticated with backend");

        let mut backend_channel = backend_session.channel_open_session().await?;

        if let Some((term, col_width, row_height, pix_width, pix_height, modes)) = &self.pty_info {
            backend_channel
                .request_pty(
                    false,
                    term,
                    *col_width,
                    *row_height,
                    *pix_width,
                    *pix_height,
                    modes,
                )
                .await?;
        }

        if let Some(cmd) = command {
            backend_channel.exec(false, cmd).await?;
            tracing::info!("Backend exec started");
        } else {
            backend_channel.request_shell(false).await?;
            tracing::info!("Backend shell started");
        }

        self.backend_session = Some(backend_session);

        let (tx, mut rx) = mpsc::unbounded_channel();
        self.client_to_backend_tx = Some(tx);

        let handle = session.handle();
        let channel_id = channel;
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    Some(data) = rx.recv() => {
                        if let Err(e) = backend_channel.data(&data[..]).await {
                            tracing::error!("Failed to send data to backend: {:?}", e);
                            break;
                        }
                    }
                    msg = backend_channel.wait() => {
                        match msg {
                            Some(russh::ChannelMsg::Data { data }) => {
                                if let Err(e) = handle.data(channel_id, data.into()).await {
                                    tracing::error!("Failed to send data to client: {:?}", e);
                                    break;
                                }
                            }
                            Some(russh::ChannelMsg::ExtendedData { data, ext }) => {
                                if let Err(e) = handle.extended_data(channel_id, ext, data.into()).await {
                                    tracing::error!("Failed to send extended data to client: {:?}", e);
                                    break;
                                }
                            }
                            Some(russh::ChannelMsg::Eof) => {
                                tracing::debug!("Backend EOF");
                                let _ = handle.eof(channel_id).await;
                            }
                            Some(russh::ChannelMsg::ExitStatus { exit_status }) => {
                                tracing::debug!("Backend exit status: {}", exit_status);
                                let _ = handle.exit_status_request(channel_id, exit_status).await;
                            }
                            Some(russh::ChannelMsg::Close) => {
                                tracing::debug!("Backend closed channel");
                                let _ = handle.close(channel_id).await;
                                break;
                            }
                            None => {
                                tracing::debug!("Backend channel stream ended");
                                let _ = handle.close(channel_id).await;
                                break;
                            }
                            _ => {}
                        }
                    }
                }
            }
        });

        Ok(())
    }
}

pub struct ClientHandler;

impl russh::client::Handler for ClientHandler {
    type Error = anyhow::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh::keys::PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}
