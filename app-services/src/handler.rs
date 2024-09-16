use anyhow::{bail, Result};
use events::Event;
use messaging::handler::MessagingMessage;
use tokio::select;
use tokio::sync::{broadcast, mpsc};
use tokio::task::{JoinHandle, JoinSet};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use crate::errors::{AppServicesError, AppServicesErrorCodes};
use crate::service::{
    await_app_service_message, create_pull_consumer, parse_settings_payload,
    reconnect_messaging_service, subscribe_to_nats, AppServiceSettings,
};
const PACKAGE_NAME: &str = env!("CARGO_CRATE_NAME");
pub struct AppServiceHandler {
    event_tx: broadcast::Sender<Event>,
    messaging_tx: mpsc::Sender<MessagingMessage>,
    app_services_consumer_token: Option<CancellationToken>,
    sync_app_services_token: Option<CancellationToken>,
}

pub enum AppServiceMessage {}

pub struct AppServiceOptions {
    pub event_tx: broadcast::Sender<Event>,
    pub messaging_tx: mpsc::Sender<MessagingMessage>,
}

impl AppServiceHandler {
    pub fn new(options: AppServiceOptions) -> Self {
        Self {
            event_tx: options.event_tx,
            messaging_tx: options.messaging_tx,
            app_services_consumer_token: None,
            sync_app_services_token: None,
        }
    }
    pub async fn subscribe_to_nats(&mut self, dns_name: String, local_port: u16) -> Result<()> {
        let fn_name = "subscribe_to_nats";
        info!(func = fn_name, package = PACKAGE_NAME, "init");

        // create a new token
        let subscriber_token = CancellationToken::new();
        let subscriber_token_cloned = subscriber_token.clone();
        let messaging_tx = self.messaging_tx.clone();
        let event_tx = self.event_tx.clone();
        let mut timer = tokio::time::interval(std::time::Duration::from_secs(50));
        let subscribers = match subscribe_to_nats(messaging_tx.clone(), dns_name.clone()).await {
            Ok(v) => v,
            Err(e) => {
                error!(
                    func = "subscribe_to_nats",
                    package = PACKAGE_NAME,
                    "subscribe to nats error - {:?}",
                    e
                );
                bail!(e)
            }
        };
        let mut futures = JoinSet::new();
        futures.spawn(await_app_service_message(
            dns_name,
            local_port,
            subscribers.request.unwrap(),
            messaging_tx.clone(),
        ));
        // create spawn for timer
        let _: JoinHandle<Result<()>> = tokio::task::spawn(async move {
            loop {
                select! {
                        _ = subscriber_token.cancelled() => {
                            info!(
                                func = fn_name,
                                package = PACKAGE_NAME,
                                result = "success",
                                "subscribe to nats cancelled"
                            );
                            return Ok(());
                    },
                    result = futures.join_next() => {
                        if result.unwrap().is_ok() {}
                    },
                    _ = timer.tick() => {}
                }
            }
            // return Ok(());
        });
        // Save to state
        // self.subscriber_token = Some(subscriber_token_cloned);
        Ok(())
    }
    // async fn app_service_consumer(&mut self, dns_name: String, local_port: u16) -> Result<bool> {
    //     let fn_name = "app_service_consumer";
    //     // safety: check for existing cancel token, and cancel it
    //     let exist_settings_token = &self.app_services_consumer_token;
    //     if exist_settings_token.is_some() {
    //         let _ = exist_settings_token.as_ref().unwrap().cancel();
    //     }
    //     // create a new token
    //     let settings_token = CancellationToken::new();
    //     let settings_token_cloned = settings_token.clone();
    //     let messaging_tx = self.messaging_tx.clone();

    //     let consumer = match create_pull_consumer(self.messaging_tx.clone(), dns_name.clone()).await
    //     {
    //         Ok(s) => s,
    //         Err(e) => {
    //             error!(
    //                 func = fn_name,
    //                 package = PACKAGE_NAME,
    //                 "error creating pull consumer, error -  {:?}",
    //                 e
    //             );
    //             bail!(AppServicesError::new(
    //                 AppServicesErrorCodes::CreateConsumerError,
    //                 format!("create consumer error - {:?} ", e.to_string()),
    //             ))
    //         }
    //     };
    //     let mut futures = JoinSet::new();
    //     futures.spawn(await_app_service_message(
    //         dns_name,
    //         local_port,
    //         consumer.clone(),
    //         messaging_tx.clone(),
    //     ));
    //     // create spawn for timer
    //     let _: JoinHandle<Result<()>> = tokio::task::spawn(async move {
    //         loop {
    //             select! {
    //                     _ = settings_token.cancelled() => {
    //                         info!(
    //                             func = fn_name,
    //                             package = PACKAGE_NAME,
    //                             result = "success",
    //                             "settings subscriber cancelled"
    //                         );
    //                         return Ok(());
    //                 },
    //                 result = futures.join_next() => {
    //                     if result.is_none() {
    //                         continue;
    //                     }
    //                 },
    //             }
    //         }
    //     });

    //     // Save to state
    //     self.app_services_consumer_token = Some(settings_token_cloned);
    //     Ok(true)
    // }
    fn clear_app_services_subscriber(&self) -> Result<bool> {
        let exist_subscriber_token = &self.app_services_consumer_token;
        if exist_subscriber_token.is_some() {
            let _ = exist_subscriber_token.as_ref().unwrap().cancel();
        } else {
            return Ok(false);
        }
        Ok(true)
    }
    fn clear_sync_settings_subscriber(&self) -> Result<bool> {
        let exist_subscriber_token = &self.sync_app_services_token;
        if exist_subscriber_token.is_some() {
            let _ = exist_subscriber_token.as_ref().unwrap().cancel();
        } else {
            return Ok(false);
        }
        Ok(true)
    }
    pub async fn run(&mut self, mut message_rx: mpsc::Receiver<AppServiceMessage>) -> Result<()> {
        let fn_name = "run";
        info!(
            func = fn_name,
            package = PACKAGE_NAME,
            "app service initiated"
        );
        let mut event_rx = self.event_tx.subscribe();
        loop {
            select! {
                msg = message_rx.recv() => {
                    if msg.is_none() {
                        continue;
                    }

                    match msg.unwrap() {};
                }

                // Receive events from other services
                event = event_rx.recv() => {
                    if event.is_err() {
                        continue;
                    }
                    match event.unwrap() {
                        Event::Messaging(events::MessagingEvent::Disconnected) => {
                            info!(
                                func = fn_name,
                                package = PACKAGE_NAME,
                                "disconnected event in app service"
                            );
                            let _ = self.clear_sync_settings_subscriber();
                            let _ = self.clear_app_services_subscriber();
                        }
                        Event::Provisioning(events::ProvisioningEvent::Deprovisioned) => {
                            info!(
                                func = fn_name,
                                package = PACKAGE_NAME,
                                "deprovisioned event in app service"
                            );
                            let _ = self.clear_sync_settings_subscriber();
                            let _ = self.clear_app_services_subscriber();
                        }
                        Event::Settings(events::SettingEvent::Updated{ existing_settings, new_settings })  => {
                            info!(
                                func = "run",
                                package = PACKAGE_NAME,
                                "settings updated event in app service"
                            );
                            //TODO: create function to handle settings update
                            for (key, value) in new_settings.into_iter() {
                                println!("************ settings updated:*************** {} / {}", key, value);
                                if key == "app_services.config" {
                                    info!(
                                        func = fn_name,
                                        package = PACKAGE_NAME,
                                        "dns_name updated event in app service: {}", value
                                    );
                                    let app_service_settings:AppServiceSettings = match parse_settings_payload(value) {
                                        Ok(s) => s,
                                        Err(e) => {
                                            error!(
                                                func = fn_name,
                                                package = PACKAGE_NAME,
                                                "error extracting req_id from key, error -  {:?}",
                                                e
                                            );
                                            bail!(e)
                                        }
                                    };
                                    info!(
                                        func = fn_name,
                                        package = PACKAGE_NAME,
                                        "dns_name updated event in app service: {} / {}", app_service_settings.dns_name, app_service_settings.app_id
                                    );
                                    if app_service_settings.dns_name == "" {
                                        // If no value then clear subscription and reconnect messaging service
                                        let _ = reconnect_messaging_service(self.messaging_tx.clone(), app_service_settings.dns_name, existing_settings.clone()).await;
                                        let _ = self.clear_app_services_subscriber();
                                        let _ = self.clear_sync_settings_subscriber();
                                    } else {
                                        // TODO: once normal flow is working, we can pass port mapping array
                                        let local_port:u16 = match app_service_settings.port_mapping[0].local_port.parse::<u16>() {
                                            Ok(s) => s,
                                            Err(e) => {
                                                error!(
                                                    func = fn_name,
                                                    package = PACKAGE_NAME,
                                                    "error extracting req_id from key, error -  {:?}",
                                                    e
                                                );
                                                bail!(AppServicesError::new(
                                                    AppServicesErrorCodes::PortParseError,
                                                    format!("error parsing local port - {:?} ", e.to_string()),
                                                ))
                                            }
                                        };
                                        let _ = reconnect_messaging_service(self.messaging_tx.clone(), app_service_settings.dns_name.clone(), existing_settings.clone()).await;
                                        let _ = self.subscribe_to_nats(app_service_settings.dns_name.clone(), local_port).await;
                                        // let _ = self.app_service_consumer(app_service_settings.dns_name, local_port).await;
                                    }
                                }
                            }
                        },
                        _ => {}

                    }
                }
            }
        }
    }
}
