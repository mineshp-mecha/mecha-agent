use std::collections::HashMap;

use crate::errors::{MessagingError, MessagingErrorCodes};
use crate::service::{get_machine_id, ConnectOptions, Messaging};
use agent_settings::constants;
use anyhow::{bail, Result};
use events::Event;
use identity::handler::IdentityMessage;
use nats_client::{jetstream::JetStreamClient, Bytes, Subscriber};
use tokio::{
    select,
    sync::{broadcast, mpsc, oneshot},
};
use tonic::async_trait;
use tracing::{error, info};

const PACKAGE_NAME: &str = env!("CARGO_PKG_NAME");

#[async_trait]
pub trait ServiceHandler {
    async fn start(&mut self) -> Result<bool>;
}
#[async_trait]
impl ServiceHandler for MessagingHandler {
    async fn start(&mut self) -> Result<bool> {
        let machine_id = match get_machine_id(self.identity_tx.clone(), false).await {
            Ok(id) => id,
            Err(e) => {
                return Ok(false);
            }
        };
        let connection_option = &self.connection_options.clone().unwrap();
        if !machine_id.is_empty() {
            match self
                .messaging_client
                .connect(
                    &self.identity_tx,
                    connection_option,
                    Some(self.event_tx.clone()),
                )
                .await
            {
                Ok(_) => {}
                Err(e) => {
                    println!("Error connecting to messaging service: {:?}", e);
                }
            };
        }
        Ok(true)
    }
}
#[derive(Clone)]
pub struct Settings {
    pub nats_addr: String,
    pub data_dir: String,
    pub service_url: String,
}
pub enum MessagingMessage {
    Connect {
        reply_to: oneshot::Sender<Result<bool>>,
    },
    Disconnect {
        reply_to: oneshot::Sender<Result<bool>>,
    },
    Reconnect {
        reply_to: oneshot::Sender<Result<bool>>,
    },
    Subscriber {
        reply_to: oneshot::Sender<Result<Subscriber>>,
        subject: String,
    },
    Send {
        reply_to: oneshot::Sender<Result<bool>>,
        message: String,
        subject: String,
        headers: Option<HashMap<String, String>>,
    },
    Request {
        reply_to: oneshot::Sender<Result<Bytes>>,
        message: String,
        subject: String,
    },
    InitJetStream {
        reply_to: oneshot::Sender<Result<JetStreamClient>>,
    },
}
pub struct MessagingOptions {
    pub settings: Settings,
    pub event_tx: broadcast::Sender<Event>,
    pub identity_tx: mpsc::Sender<IdentityMessage>,
}

pub struct MessagingHandler {
    settings: Settings,
    event_tx: broadcast::Sender<Event>,
    messaging_client: Messaging,
    identity_tx: mpsc::Sender<IdentityMessage>,
    connection_options: Option<ConnectOptions>,
}

impl MessagingHandler {
    pub fn new(options: MessagingOptions) -> Self {
        Self {
            messaging_client: Messaging::new(true, options.settings.nats_addr.clone()),
            settings: options.settings,
            event_tx: options.event_tx,
            identity_tx: options.identity_tx,
            connection_options: None,
        }
    }
    pub async fn run(&mut self, mut message_rx: mpsc::Receiver<MessagingMessage>) -> Result<()> {
        info!(
            func = "run",
            package = env!("CARGO_PKG_NAME"),
            "messaging service initiated"
        );

        let data_dir = &self.settings.data_dir.clone();
        let mut connect_options = ConnectOptions {
            nats_url: self.settings.nats_addr.clone(),
            event_tx: Some(self.event_tx.clone()),
            event_type: Some(events::MessagingEvent::Connected),
            private_key_path: (data_dir.to_owned() + constants::PRIVATE_KEY_PATH),
            service_url: self.settings.service_url.clone(),
            get_nonce_url: constants::NONCE_URL_QUERY_PATH.to_string(),
            issue_auth_token_url: constants::ISSUE_TOKEN_URL_QUERY_PATH.to_string(),
        };

        let connection_opts = connect_options.clone();
        self.connection_options = Some(connect_options.clone());
        let _ = &self.start().await;
        let mut event_rx = self.event_tx.subscribe();
        loop {
            select! {
                msg = message_rx.recv() => {
                    if msg.is_none() {
                        continue;
                    }
                    match msg.unwrap() {
                        MessagingMessage::Send{reply_to, message, subject, headers} => {
                            let res = self.messaging_client.publish(&subject.as_str(), headers, Bytes::from(message)).await;
                            let _ = reply_to.send(res);
                        }
                        MessagingMessage::Request{reply_to, message, subject} => {
                            let res = self.messaging_client.request(&subject.as_str(), Bytes::from(message)).await;
                            let _ = reply_to.send(res);
                        },
                        MessagingMessage::Connect { reply_to } => {
                            let res = self.messaging_client.connect(&self.identity_tx, &connection_opts, Some(self.event_tx.clone())).await;
                            let _ = reply_to.send(res);
                        },
                        MessagingMessage::Reconnect { reply_to } => {
                            //Override event_type value
                            connect_options = ConnectOptions {
                                event_type: Some(events::MessagingEvent::Reconnected),
                                ..connection_opts.clone()
                            };
                            let res = self.messaging_client.connect(&self.identity_tx, &connect_options, Some(self.event_tx.clone())).await;
                            let _ = reply_to.send(res);
                        },
                        MessagingMessage::Subscriber { reply_to, subject } => {
                            let res = self.messaging_client.subscribe(subject.as_str()).await;
                            let _ = reply_to.send(res);
                        },
                        MessagingMessage::InitJetStream { reply_to } => {
                            let res = self.messaging_client.init_jetstream().await;
                            let _ = reply_to.send(res);
                        }
                        _ => {
                        }
                    };
                }
                // Receive events from other services
                event = event_rx.recv() => {
                    if event.is_err() {
                        continue;
                    }
                    match event.unwrap() {
                        Event::Provisioning(events::ProvisioningEvent::Provisioned) => {
                            info!(
                                func = "run",
                                package = env!("CARGO_PKG_NAME"),
                                "messaging service received provisioning event"
                            );
                            match self.messaging_client.connect(&self.identity_tx, &connect_options, Some(self.event_tx.clone())).await{
                                Ok(_) => (),
                                Err(e) => {
                                    error!(
                                        func = "run",
                                        package = PACKAGE_NAME,
                                        "error starting messaging service - {}",
                                        e
                                    );
                                }
                            }

                        },
                        Event::Provisioning(events::ProvisioningEvent::Deprovisioned) => {
                            info!(
                                func = "run",
                                package = env!("CARGO_PKG_NAME"),
                                "messaging service received deprovisioning event"
                            );
                        },
                        Event::Nats(nats_client::NatsEvent::Disconnected) => {
                            let _ = match self.event_tx.send(Event::Messaging(events::MessagingEvent::Disconnected)) {
                                Ok(_) => {}
                                Err(e) => {
                                    error!(
                                        func = "run",
                                        package = PACKAGE_NAME,
                                        "error sending messaging service disconnected event - {}",
                                        e
                                    );
                                    bail!(MessagingError::new(
                                        MessagingErrorCodes::EventSendError,
                                        format!("error sending messaging service disconnected - {}", e),
                                    ));
                                }
                            };
                            //Override event_type value
                            connect_options = ConnectOptions {
                                event_type: Some(events::MessagingEvent::Connected),
                                ..connection_opts.clone()
                            };
                            match self.messaging_client.connect(&self.identity_tx, &connect_options, Some(self.event_tx.clone())).await {
                                Ok(_) => (),
                                Err(e) => {
                                    error!(
                                        func = "run",
                                        package = PACKAGE_NAME,
                                        "error starting messaging service - {}",
                                        e
                                    );
                                }
                            }
                        },
                        Event::Nats(nats_client::NatsEvent::Connected) => {
                            //This will help in re-connecting all the services
                            match self.event_tx.send(Event::Messaging(events::MessagingEvent::Connected)) {
                                Ok(_) => {}
                                Err(e) => {
                                    error!(
                                        func = "run",
                                        package = PACKAGE_NAME,
                                        "error sending messaging service connected event - {}",
                                        e
                                    );
                                }
                            }
                        },
                        Event::Nats(nats_client::NatsEvent::ServerError(err)) => {
                            error!(
                                func = "run",
                                package = PACKAGE_NAME,
                                "messaging service received nats server error - {}",
                                err
                            );
                        },
                      _ => {}
                    }
                }
            }
        }
    }
}
