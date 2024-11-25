use std::collections::HashMap;

use crate::errors::{MessagingError, MessagingErrorCodes};
use crate::service::{
    connect, get_machine_id, init_jetstream, new, publish, request, subscribe, ConnectOptions,
};
use agent_settings::{constants, read_settings_yml, AgentSettings};
use anyhow::{bail, Result};
use events::Event;
use identity::handler::IdentityMessage;
use nats_client::NatsClient;
use nats_client::{jetstream::JetStreamClient, Bytes, Subscriber};
use services::{ServiceHandler, ServiceStatus};
use tokio::{
    select,
    sync::{broadcast, mpsc, oneshot},
};
use tonic::async_trait;
use tracing::{error, info, warn};

const PACKAGE_NAME: &str = env!("CARGO_PKG_NAME");

pub enum MessagingMessage {
    Connect {
        reply_to: oneshot::Sender<Result<NatsClient>>,
    },
    Disconnect {
        reply_to: oneshot::Sender<Result<bool>>,
    },
    Reconnect {
        reply_to: oneshot::Sender<Result<NatsClient>>,
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
    pub nats_addr: String,
    pub event_tx: broadcast::Sender<Event>,
    pub identity_tx: mpsc::Sender<IdentityMessage>,
}

pub struct MessagingHandler {
    event_tx: broadcast::Sender<Event>,
    status: ServiceStatus,
    messaging_client: Option<NatsClient>,
    identity_tx: mpsc::Sender<IdentityMessage>,
}

impl MessagingHandler {
    pub fn new(options: MessagingOptions) -> Self {
        Self {
            event_tx: options.event_tx,
            status: ServiceStatus::STARTED,
            messaging_client: new(options.nats_addr),
            identity_tx: options.identity_tx,
        }
    }
    pub async fn run(
        &mut self,
        mut connect_options: Option<ConnectOptions>,
        mut message_rx: mpsc::Receiver<MessagingMessage>,
    ) -> Result<()> {
        info!(
            func = "run",
            package = env!("CARGO_PKG_NAME"),
            "messaging service initiated"
        );
        if connect_options.is_none() {
            let settings = match read_settings_yml(None) {
                Ok(s) => s,
                Err(e) => {
                    warn!(
                        func = "run",
                        package = PACKAGE_NAME,
                        "error reading settings - {}",
                        e
                    );
                    AgentSettings::default()
                }
            };
            connect_options = Some(ConnectOptions {
                machine_id: None,
                event_tx: Some(self.event_tx.clone()),
                event_type: Some(events::MessagingEvent::Connected),
                private_key_path: settings.data.dir + constants::PRIVATE_KEY_PATH,
                service_url: settings.backend.service,
                get_nonce_url: constants::NONCE_URL_QUERY_PATH.to_string(),
                issue_auth_token_url: constants::ISSUE_TOKEN_URL_QUERY_PATH.to_string(),
            });
        }
        connect_options = match self.start(connect_options.unwrap()).await {
            Ok(res) => Some(res),
            Err(e) => {
                error!(
                    func = "run",
                    package = PACKAGE_NAME,
                    "error starting messaging service - {}",
                    e
                );
                bail!(MessagingError::new(
                    MessagingErrorCodes::ServiceStartError,
                    format!("error starting messaging service - {}", e),
                ));
            }
        };
        let connection_opts = connect_options.unwrap().clone();
        let mut event_rx = self.event_tx.subscribe();
        loop {
            select! {
                msg = message_rx.recv() => {
                    if msg.is_none() {
                        continue;
                    }
                    match msg.unwrap() {
                        MessagingMessage::Send{reply_to, message, subject, headers} => {
                            let res = publish(self.messaging_client.clone(), &subject.as_str(), headers, Bytes::from(message)).await;
                            let _ = reply_to.send(res);
                        }
                        MessagingMessage::Request{reply_to, message, subject} => {
                            let res = request(self.messaging_client.clone(), &subject.as_str(), Bytes::from(message)).await;
                            let _ = reply_to.send(res);
                        },
                        MessagingMessage::Connect { reply_to } => {
                            let res = connect(self.messaging_client.clone(), &connection_opts).await;
                            let _ = reply_to.send(res);
                        },
                        MessagingMessage::Reconnect { reply_to } => {
                            //Override event_type value
                            connect_options = Some(ConnectOptions {
                                event_type: Some(events::MessagingEvent::Reconnected),
                                ..connection_opts.clone()
                            });
                            let res = connect(self.messaging_client.clone(), &connect_options.unwrap()).await;
                            let _ = reply_to.send(res);
                        },
                        MessagingMessage::Subscriber { reply_to, subject } => {
                            let res = subscribe(self.messaging_client.clone(), subject.as_str()).await;
                            let _ = reply_to.send(res);
                        },
                        MessagingMessage::InitJetStream { reply_to } => {
                            let res = init_jetstream(self.messaging_client.clone()).await;
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
                            let _ = &self.start(connection_opts.clone()).await;
                        },
                        Event::Provisioning(events::ProvisioningEvent::Deprovisioned) => {
                            info!(
                                func = "run",
                                package = env!("CARGO_PKG_NAME"),
                                "messaging service received deprovisioning event"
                            );
                            let _ = &self.stop().await;
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
                            connect_options = Some(ConnectOptions {
                                event_type: Some(events::MessagingEvent::Reconnected),
                                ..connection_opts.clone()
                            });
                            let _ = connect(self.messaging_client.clone(), &connect_options.unwrap()).await;
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

#[async_trait]
impl ServiceHandler<ConnectOptions> for MessagingHandler {
    async fn start(&mut self, mut connect_options: ConnectOptions) -> Result<ConnectOptions> {
        let machine_id_str: String;
        if connect_options.machine_id.is_none() {
            machine_id_str = match get_machine_id(self.identity_tx.clone(), true).await {
                Ok(id) => id,
                Err(e) => bail!(e),
            };
        } else {
            machine_id_str = connect_options.machine_id.clone().unwrap();
        }

        if !machine_id_str.is_empty() {
            self.status = ServiceStatus::STARTED;
            connect_options.machine_id = Some(machine_id_str);
            match connect(self.messaging_client.clone(), &connect_options).await {
                Ok(connected_client) => {
                    self.messaging_client = Some(connected_client);
                }
                Err(e) => {
                    println!("error connecting nats client: {:?}", e);
                }
            };
        }
        Ok(connect_options)
    }

    async fn stop(&mut self) -> Result<bool> {
        self.status = ServiceStatus::STOPPED;
        Ok(true)
    }

    fn get_status(&self) -> anyhow::Result<ServiceStatus> {
        Ok(self.status)
    }

    fn is_stopped(&self) -> Result<bool> {
        Ok(self.status == ServiceStatus::STOPPED)
    }

    fn is_started(&self) -> Result<bool> {
        Ok(self.status == ServiceStatus::STARTED)
    }
}
