use crate::{
    metrics::initialize_metrics,
    service::{process_logs, process_metrics},
};
use anyhow::Result;
use events::Event;
use identity::handler::IdentityMessage;
use messaging::handler::MessagingMessage;
use settings::handler::SettingMessage;
use tokio::{
    select,
    sync::{broadcast, mpsc, oneshot},
};
use tracing::{error, info};

pub struct Settings {
    pub export_endpoint: String,
}
pub struct TelemetryHandler {
    pub settings: Settings,
    event_tx: broadcast::Sender<Event>,
    pub messaging_tx: mpsc::Sender<MessagingMessage>,
    pub identity_tx: mpsc::Sender<IdentityMessage>,
    pub settings_tx: mpsc::Sender<SettingMessage>,
    is_messaging_connected: bool,
}
pub struct TelemetryOptions {
    pub settings: Settings,
    pub event_tx: broadcast::Sender<Event>,
    pub messaging_tx: mpsc::Sender<MessagingMessage>,
    pub identity_tx: mpsc::Sender<IdentityMessage>,
    pub settings_tx: mpsc::Sender<SettingMessage>,
}

pub enum TelemetryMessage {
    SendLogs {
        logs: Vec<u8>,
        logs_type: String,
        reply_to: oneshot::Sender<Result<bool>>,
    },
    SendMetrics {
        metrics: Vec<u8>,
        metrics_type: String,
        reply_to: oneshot::Sender<Result<bool>>,
    },
}

impl TelemetryHandler {
    pub fn new(options: TelemetryOptions) -> Self {
        Self {
            settings: options.settings,
            event_tx: options.event_tx,
            identity_tx: options.identity_tx,
            messaging_tx: options.messaging_tx,
            settings_tx: options.settings_tx,
            is_messaging_connected: false,
        }
    }
    pub async fn run(&mut self, mut message_rx: mpsc::Receiver<TelemetryMessage>) -> Result<()> {
        info!(
            func = "run",
            package = env!("CARGO_PKG_NAME"),
            "telemetry service initiated"
        );
        // Start the service
        let mut event_rx = self.event_tx.subscribe();

        loop {
            select! {
                msg = message_rx.recv() => {
                    if msg.is_none() {
                        continue;
                    }

                    match msg.unwrap() {
                        TelemetryMessage::SendLogs {logs, logs_type, reply_to } => {
                            if self.is_messaging_connected {
                                let result = process_logs(logs_type, logs, self.identity_tx.clone(), self.messaging_tx.clone(), self.settings_tx.clone() ).await;
                                let _ = reply_to.send(result);
                            }
                        }
                        TelemetryMessage::SendMetrics {metrics, metrics_type, reply_to } => {
                            if self.is_messaging_connected {
                                let result = process_metrics(metrics, metrics_type, self.identity_tx.clone(), self.messaging_tx.clone(), self.settings_tx.clone()).await;
                                let _ = reply_to.send(result);
                            }

                        }
                    };
                }
              // Receive events from other services
              event = event_rx.recv() => {
                if event.is_err() {
                    continue;
                }
                match event.unwrap() {
                    Event::Messaging(events::MessagingEvent::Connected) => {
                        self.is_messaging_connected = true;
                        match initialize_metrics(&self.settings.export_endpoint).await {
                            Ok(_) => {
                                info!(
                                    func = "run",
                                    package = env!("CARGO_PKG_NAME"),
                                    "metrics initialized"
                                );
                            },
                            Err(e) => error!(
                                func = "run",
                                package = env!("CARGO_PKG_NAME"),
                                "failed to initialize metrics - {:?}",e
                            ),
                        }
                    },
                    Event::Messaging(events::MessagingEvent::Disconnected) => {
                        self.is_messaging_connected = false;
                    },
                    Event::Provisioning(events::ProvisioningEvent::Deprovisioned) => {
                        self.is_messaging_connected = false;
                    },
                    _ => {}
                }
            }
            }
        }
    }
}
