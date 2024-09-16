use std::{collections::HashMap, io::Read, sync::Arc};

use agent_settings::AgentSettings;
use anyhow::{bail, Result};
use channel::recv_with_timeout;
use crypto::random::generate_random_alphanumeric;
use futures::StreamExt;
use http_body_util::BodyExt;
use messaging::{async_nats::Message, Subscriber as NatsSubscriber};

use hyper::{
    body::{Bytes as HyperBodyBytes, Incoming},
    Response,
};
use messaging::{async_nats::HeaderMap, handler::MessagingMessage};
use nats_client::{
    async_nats::jetstream::consumer::{pull::Config, Consumer},
    Bytes,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha256::digest;
use tokio::sync::{
    mpsc::{self, Sender},
    oneshot, Mutex,
};
use tokio_util::bytes::{BufMut, BytesMut};
use tracing::{debug, error, info, trace, warn};

use crate::{
    errors::{AppServicesError, AppServicesErrorCodes},
    hyper_client,
};
const PACKAGE_NAME: &str = env!("CARGO_PKG_NAME");

#[derive(Debug, Deserialize)]
pub struct IncomingHttpRequest {
    pub uri: String,
    pub method: String,
    pub req_id: String,
    pub headers: std::collections::HashMap<String, String>,
}
#[derive(Serialize, Deserialize, Debug)]
pub struct SettingsAckPayload {
    pub status: String,
}

#[derive(Debug)]
pub enum AppServicesSubjects {
    Request(String),
}

#[derive(Debug, Default)]
pub struct ProvisioningSubscriber {
    pub request: Option<NatsSubscriber>,
}
#[derive(Serialize, Deserialize, Debug)]
pub struct AddTaskRequestPayload {
    pub key: String,
    pub value: String,
    pub created_at: String,
}
#[derive(Serialize, Deserialize, Clone)]
pub struct DeviceSettings {
    settings: AgentSettings,
}

#[derive(Debug, Deserialize)]
pub struct AppServiceSettings {
    pub app_id: String,
    pub app_name: String,
    pub dns_name: String,
    pub port_mapping: Vec<PortMapping>,
}

#[derive(Debug, Deserialize)]
pub struct PortMapping {
    pub local_port: String,
    pub target_port: String,
    pub protocol: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ResponseBody {
    pub headers: std::collections::HashMap<String, String>,
    pub body: HyperBodyBytes,
}

type ReqTx = mpsc::Sender<String>;
#[derive()]
struct RequestState {
    tx: ReqTx,
    uri: String,
    req_headers: HashMap<String, String>,
    req_method: String,
    req_body: BytesMut,
    content_length: usize,
}

pub async fn subscribe_to_nats(
    messaging_tx: mpsc::Sender<MessagingMessage>,
    dns_name: String,
) -> Result<ProvisioningSubscriber> {
    let fn_name = "subscribe_to_nats";
    let list_of_subjects = vec![AppServicesSubjects::Request(format!(
        "app_services.gateway.{}.>",
        sha256::digest(dns_name)
    ))];
    debug!(
        func = fn_name,
        package = PACKAGE_NAME,
        "list of subjects - {:?}",
        list_of_subjects
    );
    let mut provisioning_subscribers = ProvisioningSubscriber::default();
    // Iterate over everything.
    for subject in list_of_subjects {
        let (tx, rx) = oneshot::channel();
        let subject_string = match &subject {
            AppServicesSubjects::Request(s) => s.to_string(),
        };
        match messaging_tx
            .send(MessagingMessage::Subscriber {
                reply_to: tx,
                subject: subject_string,
            })
            .await
        {
            Ok(_) => {}
            Err(e) => {
                error!(
                    func = fn_name,
                    package = PACKAGE_NAME,
                    "error sending get que subscriber for issue token- {}",
                    e
                );
                bail!(e)
            }
        }
        match recv_with_timeout(rx).await {
            Ok(subscriber) => match &subject {
                &AppServicesSubjects::Request(_) => {
                    provisioning_subscribers.request = Some(subscriber)
                }
            },
            Err(e) => {
                error!(
                    func = fn_name,
                    package = PACKAGE_NAME,
                    "error while get networking subscriber - {:?}, error - {}",
                    &subject,
                    e
                );
                bail!(e)
            }
        };
    }

    Ok(provisioning_subscribers)
}
pub async fn create_pull_consumer(
    messaging_tx: Sender<MessagingMessage>,
    dns_name: String,
) -> Result<Consumer<Config>> {
    let fn_name = "create_pull_consumer";
    let (tx, rx) = oneshot::channel();
    match messaging_tx
        .send(MessagingMessage::InitJetStream { reply_to: tx })
        .await
    {
        Ok(_) => {}
        Err(err) => {
            error!(
                func = fn_name,
                package = PACKAGE_NAME,
                "error sending init jetstream message - {:?}",
                err
            );
            bail!(AppServicesError::new(
                AppServicesErrorCodes::ChannelSendMessageError,
                format!(
                    "error sending init jetstream message - {:?}",
                    err.to_string()
                ),
            ))
        }
    }

    let jet_stream_client = match recv_with_timeout(rx).await {
        Ok(js_client) => js_client,
        Err(err) => {
            error!(
                func = fn_name,
                package = PACKAGE_NAME,
                "error receiving init jetstream message - {:?}",
                err
            );
            bail!(AppServicesError::new(
                AppServicesErrorCodes::ChannelReceiveMessageError,
                format!("error receiving init jetstream message - {:?}", err),
            ))
        }
    };
    println!("jet_stream client received");
    let stream_name = "app_services";
    let stream = match jet_stream_client.get_stream(stream_name.to_string()).await {
        Ok(s) => s,
        Err(e) => {
            error!(
                func = fn_name,
                package = PACKAGE_NAME,
                "error getting stream, name - {}, error -  {:?}",
                stream_name,
                e
            );
            bail!(e)
        }
    };
    // Create consumer
    let consumer_name = generate_random_alphanumeric(10);
    debug!(
        func = fn_name,
        package = PACKAGE_NAME,
        "consumer name generated - {}",
        &consumer_name
    );

    // Get DNS name from settings
    //app_services.gateway.(sha256_of_dns_name).{port).req
    let filter_subject = format!("app_services.gateway.{}.>", digest(dns_name));
    info!(
        func = fn_name,
        package = PACKAGE_NAME,
        "creating consumer, name - {}, filter_subject - {}",
        &consumer_name,
        &filter_subject
    );
    let consumer = match jet_stream_client
        .create_consumer(stream, filter_subject, consumer_name.clone())
        .await
    {
        Ok(s) => s,
        Err(e) => {
            error!(
                func = fn_name,
                package = PACKAGE_NAME,
                "error creating consumer, name - {}, error -  {:?}",
                &consumer_name,
                e
            );
            bail!(e)
        }
    };
    info!(func = fn_name, package = PACKAGE_NAME, "consumer created");
    Ok(consumer)
}

pub async fn await_app_service_message(
    dns_name: String,
    local_port: u16,
    mut subscriber: NatsSubscriber,
    messaging_tx: Sender<MessagingMessage>,
) -> Result<bool> {
    let fn_name = "await_app_service_message";
    println!("awaiting app service message");

    // Create a HashMap wrapped in a Mutex and Arc for shared ownership
    let req_map: Arc<Mutex<HashMap<String, RequestState>>> = Arc::new(Mutex::new(HashMap::new()));
    // let mut messages = match consumer.messages().await {
    //     Ok(s) => s,
    //     Err(e) => {
    //         error!(
    //             func = fn_name,
    //             package = PACKAGE_NAME,
    //             "error fetching messages, error -  {:?}",
    //             e
    //         );
    //         bail!(AppServicesError::new(
    //             AppServicesErrorCodes::PullMessagesError,
    //             format!("pull messages error - {:?} - {}", e.kind(), e.to_string()),
    //         ))
    //     }
    // };

    println!("messages fetched");
    // while let Some(Ok(message)) = subscriber.next().await {
    while let Some(message) = subscriber.next().await {
        println!("new message subject: {:?}", message.subject);
        let message_tx_cloned = messaging_tx.clone();
        let dns_name_cloned = dns_name.clone();
        // Spawn a task that will simulate adding new requests in a loop
        let req_map_clone = Arc::clone(&req_map);
        // Spawn a tokio task to serve multiple requests concurrently
        tokio::task::spawn(async move {
            match process_message(
                dns_name_cloned,
                local_port,
                message.clone(),
                message_tx_cloned,
                req_map_clone,
            )
            .await
            {
                Ok(_) => {}
                Err(err) => {
                    error!(
                        func = fn_name,
                        package = PACKAGE_NAME,
                        "error processing message - {:?}",
                        err
                    );
                }
            }
            // Acknowledges a message delivery
            // match message.ack().await {
            //     Ok(_res) => info!(
            //         func = fn_name,
            //         package = PACKAGE_NAME,
            //         "message Acknowledged",
            //     ),
            //     Err(err) => {
            //         error!(
            //             func = fn_name,
            //             package = PACKAGE_NAME,
            //             "message acknowledge failed {}",
            //             err
            //         );
            //     }
            // };
        });
    }
    Ok(true)
}

async fn process_message(
    dns_name: String,
    local_port: u16,
    message: Message,
    messaging_tx: Sender<MessagingMessage>,
    req_map_clone: Arc<Mutex<HashMap<String, RequestState>>>,
) -> Result<bool> {
    let fn_name = "process_message";
    debug!(
        func = fn_name,
        package = PACKAGE_NAME,
        "processing message - {:?}",
        message
    );
    let client_response = if message.subject.ends_with(".req") {
        info!(
            func = fn_name,
            package = PACKAGE_NAME,
            "message is a request"
        );
        // Process message payload
        let http_incoming_request = match parse_message_payload(&message.payload) {
            Ok(s) => s,
            Err(err) => {
                error!(
                    func = fn_name,
                    package = PACKAGE_NAME,
                    "error parsing message payload - {:?}",
                    err
                );
                bail!(err)
            }
        };
        info!(
            func = fn_name,
            package = PACKAGE_NAME,
            "message processed successfully"
        );

        // Find key content-length from headers
        let content_length = match http_incoming_request.headers.get("content-length") {
            Some(s) => match s.parse::<usize>() {
                // TODO: check what if length is bigger than this
                Ok(s) => s,
                Err(e) => {
                    error!(
                        func = fn_name,
                        package = PACKAGE_NAME,
                        "error parsing content-length - {:?}",
                        e
                    );
                    bail!(e)
                }
            },
            None => 0,
        };
        info!(
            func = fn_name,
            package = PACKAGE_NAME,
            "content length - {:?}",
            content_length
        );
        let mut map = req_map_clone.lock().await;
        let (tx, rx) = mpsc::channel(10);
        let request_state = RequestState {
            tx: tx,
            uri: http_incoming_request.uri.clone(),
            req_headers: http_incoming_request.headers.clone(),
            req_method: http_incoming_request.method.clone(),
            req_body: BytesMut::with_capacity(content_length),
            content_length: content_length,
        };
        map.insert(http_incoming_request.req_id.clone(), request_state);
        // If content length is greater than zero it means next message will have req.data with payload
        let client_response: Option<Response<Incoming>> = if content_length <= 0 {
            // Send request to local service
            let client_response: Response<Incoming> =
                match handle_local_request(http_incoming_request, local_port).await {
                    Ok(s) => s,
                    Err(err) => {
                        error!(
                            func = fn_name,
                            package = PACKAGE_NAME,
                            "error handling local request - {:?}",
                            err
                        );
                        bail!(err)
                    }
                };
            Some(client_response)
        } else {
            None
        };
        client_response
    } else if message.subject.ends_with(".data") {
        let req_id = match extract_req_id_from_subject(&message.subject.to_string()) {
            Ok(req_id) => req_id.to_string(),
            Err(err) => bail!(err),
        };
        // Lock the mutex to get a value by key
        let mut map_lock = req_map_clone.lock().await;
        let client_response_opt = if let Some(existing_req) = map_lock.get_mut(&req_id) {
            let response: Option<Response<Incoming>> = match handle_request_with_content(
                existing_req,
                message.payload.clone(),
                local_port,
            )
            .await
            {
                Ok(s) => s,
                Err(err) => {
                    error!(
                        func = fn_name,
                        package = PACKAGE_NAME,
                        "error handling incoming request - {:?}",
                        err
                    );
                    bail!(err)
                }
            };
            response
        } else {
            warn!(
                func = fn_name,
                package = PACKAGE_NAME,
                "request not found in map - {:?}",
                req_id
            );
            None
        };
        client_response_opt
    } else {
        None
    };

    if client_response.is_some() {
        let client_response = client_response.unwrap();
        // Collect response headers
        let headers = client_response
            .headers()
            .iter()
            .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
            .collect::<std::collections::HashMap<String, String>>();
        info!(
            func = fn_name,
            package = PACKAGE_NAME,
            "response headers - {:?}",
            headers
        );
        let response_body_bytes = client_response.collect().await.unwrap().to_bytes();
        // Specify the header name you want to retrieve
        let ack_subject = match extract_ack_subject(message.headers.as_ref()) {
            Ok(s) => s,
            Err(err) => bail!(err),
        };
        if !ack_subject.is_empty() {
            let response_payload = json!(ResponseBody {
                headers: headers,
                body: response_body_bytes,
            });

            let (tx, _rx) = oneshot::channel();
            match messaging_tx
                .send(MessagingMessage::Send {
                    reply_to: tx,
                    message: response_payload.to_string(),
                    subject: ack_subject,
                    headers: None,
                })
                .await
            {
                Ok(_) => {
                    info!(
                        func = fn_name,
                        package = PACKAGE_NAME,
                        "response sent to nats"
                    );
                }
                Err(e) => {
                    error!(
                        func = fn_name,
                        package = PACKAGE_NAME,
                        "error sending response to nats - {:?}",
                        e
                    );
                }
            }
        }
    }
    Ok(true)
}

fn extract_ack_subject(headers: Option<&HeaderMap>) -> Result<String> {
    let fn_name = "extract_ack_subject";
    let header_name = "Ack-To";
    let header_map_values = match &headers {
        Some(header_map) => header_map,
        None => {
            error!(
                func = fn_name,
                package = PACKAGE_NAME,
                "message doesn't contain any headers"
            );
            bail!(AppServicesError::new(
                AppServicesErrorCodes::MessageHeaderEmptyError,
                format!("message doesn't contain any headers"),
            ))
        }
    };
    if let Some(header_value) = header_map_values.get(header_name) {
        return Ok(header_value.to_string());
    } else {
        error!(
            func = fn_name,
            package = PACKAGE_NAME,
            "message doesn't contain Ack-To header"
        );
        bail!(AppServicesError::new(
            AppServicesErrorCodes::AckHeaderNotFoundError,
            format!("message doesn't contain Ack-To header"),
        ))
    }
}

fn extract_req_id_from_subject(subject: &str) -> Result<&str> {
    let parts: Vec<&str> = subject.split('.').collect();

    // Check if the vector has at least 6 parts (to include "app_service.gateway.dns_name.port.req_id.req")
    if parts.len() >= 6 {
        return Ok(parts[4]); // "req_id" is at index 4
    }
    bail!(AppServicesError::new(
        AppServicesErrorCodes::ReqIdParseError,
        format!("error parsing subject - {}", subject),
    ))
}

async fn handle_request_with_content(
    existing_request: &mut RequestState,
    new_data_chunks: Bytes,
    local_port: u16,
) -> Result<Option<Response<Incoming>>> {
    let fn_name = "handle_request_with_content";
    let mut buf = existing_request.req_body.clone();
    info!(
        func = fn_name,
        package = PACKAGE_NAME,
        "buffer length before new chunks added - {}",
        buf.len()
    );
    buf.put(new_data_chunks.clone());
    info!(
        func = fn_name,
        package = PACKAGE_NAME,
        "buffer length after new chunks added - {}",
        buf.len()
    );
    if buf.len() == existing_request.content_length {
        info!(
            func = fn_name,
            package = PACKAGE_NAME,
            "payload length and content length is equal now"
        );
        let url = match format!("http://localhost:{}{}", local_port, existing_request.uri)
            .parse::<hyper::Uri>()
        {
            Ok(s) => s,
            Err(e) => {
                error!(
                    func = fn_name,
                    package = PACKAGE_NAME,
                    "error parsing url - {:?}",
                    e
                );
                bail!(e)
            }
        };
        // Get the host and the port
        let host = url.host().expect("uri has no host");
        let port = url.port_u16().unwrap_or(80);
        let remote_host_addr = format!("{}:{}", host, port);
        // Parse and process message
        let response = match hyper_client::make_request_with_body(
            remote_host_addr,
            url,
            existing_request.req_headers.clone(),
            existing_request.req_method.clone(),
            Bytes::from(buf.clone()),
        )
        .await
        {
            Ok(resp) => resp,
            Err(err) => {
                error!(func = fn_name, package = PACKAGE_NAME, "");
                bail!(err)
            }
        };
        return Ok(Some(response));
    } else {
        warn!(
            func = fn_name,
            package = PACKAGE_NAME,
            "payload length and content length is not equal yet {}:{}",
            buf.len(),
            existing_request.content_length
        );
        existing_request.req_body = buf;
        return Ok(None);
    }
}
async fn handle_local_request(
    http_incoming_request: IncomingHttpRequest,
    local_port: u16,
) -> Result<Response<Incoming>> {
    let fn_name = "handle_local_request";
    debug!(
        func = fn_name,
        package = PACKAGE_NAME,
        "handling local request - {:?}",
        http_incoming_request
    );

    let url = match format!(
        "http://localhost:{}{}",
        local_port, http_incoming_request.uri
    )
    .parse::<hyper::Uri>()
    {
        Ok(s) => s,
        Err(e) => {
            error!(
                func = fn_name,
                package = PACKAGE_NAME,
                "error parsing url - {:?}",
                e
            );
            bail!(e)
        }
    };
    // Get the host and the port
    let host = url.host().expect("uri has no host");
    let port = url.port_u16().unwrap_or(80);
    let remote_host_addr = format!("{}:{}", host, port);
    let response = match hyper_client::make_request_without_body(
        remote_host_addr,
        url,
        http_incoming_request.headers,
    )
    .await
    {
        Ok(resp) => resp,
        Err(err) => bail!(err),
    };
    Ok(response)
    // To make this streamed - fetch all data frames
    // while let Some(item) = response.frame().await {
    //     let (tx, _rx) = oneshot::channel();
    //     match item {
    //         Ok(frame) => {
    //             if frame.is_data() {
    //                 let chunk = frame.into_data().unwrap().to_vec();
    //                 println!("new chunk - {:?}", chunk.len());
    //                 match messaging_tx
    //                     .send(MessagingMessage::Send {
    //                         reply_to: tx,
    //                         message: json!(ack_payload).to_string(),
    //                         subject: header_value.to_string(),
    //                         headers: None,
    //                     })
    //                     .await
    //                 {
    //                     Ok(_) => {
    //                         println!("published to nats");
    //                     }
    //                     Err(e) => {
    //                         println!("error while publishing to nats: {:?}", e);
    //                     }
    //                 }
    //             } else if frame.is_trailers() {
    //                 let trailers = frame.into_data().unwrap();
    //                 println!("trailers - {:?}", trailers.len());
    //             }
    //         }
    //         Err(err) => {
    //             return Err(err);
    //         }
    //     }
    // }
}

fn parse_message_payload(payload: &Bytes) -> Result<IncomingHttpRequest> {
    debug!(
        func = "parse_message_payload",
        package = PACKAGE_NAME,
        "parsing message payload - {:?}",
        payload
    );
    let payload_value = match std::str::from_utf8(&payload) {
        Ok(s) => s,
        Err(e) => {
            error!(
                func = "parse_message_payload",
                package = PACKAGE_NAME,
                "error parsing message payload - {:?}",
                e
            );
            bail!(e)
        }
    };
    let payload: IncomingHttpRequest = match serde_json::from_str(payload_value) {
        Ok(s) => s,
        Err(e) => {
            error!(
                func = "parse_message_payload",
                package = PACKAGE_NAME,
                "error converting payload to incoming http request - {:?}",
                e
            );
            bail!(AppServicesError::new(
                AppServicesErrorCodes::RequestPayloadParseError,
                format!("error converting payload to incomingHttpRequest - {}", e),
            ))
        }
    };
    info!(
        func = "parse_message_payload",
        package = PACKAGE_NAME,
        "payload parsed",
    );
    Ok(payload)
}

pub async fn reconnect_messaging_service(
    messaging_tx: Sender<MessagingMessage>,
    new_setting: String,
    existing_settings: HashMap<String, String>,
) -> Result<bool> {
    let fn_name = "reconnect_messaging_service";
    match existing_settings.get("app_services.{app_id}.dns_name") {
        Some(setting) => {
            if setting == &new_setting {
                info!(
                    func = fn_name,
                    package = PACKAGE_NAME,
                    "networking settings are same, no need to reconnect"
                );
                return Ok(true);
            }
        }
        None => {
            info!(
                func = fn_name,
                package = PACKAGE_NAME,
                "existing networking settings not found, reconnecting"
            );
        }
    }
    let (tx, rx) = oneshot::channel();
    match messaging_tx
        .send(MessagingMessage::Reconnect { reply_to: tx })
        .await
    {
        Ok(_) => {}
        Err(e) => {
            error!(
                func = fn_name,
                package = PACKAGE_NAME,
                "error sending reconnect message - {}",
                e
            );
            bail!(AppServicesError::new(
                AppServicesErrorCodes::SendReconnectMessagingMessageError,
                format!("error sending reconnect message - {}", e),
            ));
        }
    }
    let result = match recv_with_timeout(rx).await {
        Ok(res) => res,
        Err(e) => {
            error!(
                func = fn_name,
                package = PACKAGE_NAME,
                "error receiving reconnect message - {}",
                e
            );
            bail!(AppServicesError::new(
                AppServicesErrorCodes::RecvReconnectMessageError,
                format!("error receiving reconnect message - {}", e),
            ));
        }
    };
    info!(
        func = fn_name,
        package = PACKAGE_NAME,
        "reconnect request completed",
    );
    Ok(result)
}

pub fn parse_settings_payload(payload: String) -> Result<AppServiceSettings> {
    let fn_name = "parse_settings_payload";
    debug!(
        func = fn_name,
        package = PACKAGE_NAME,
        "parsing message payload - {:?}",
        payload
    );
    let payload: AppServiceSettings = match serde_json::from_str(&payload) {
        Ok(s) => s,
        Err(e) => {
            error!(
                func = fn_name,
                package = PACKAGE_NAME,
                "error converting payload to app service settings - {:?}",
                e
            );
            bail!(AppServicesError::new(
                AppServicesErrorCodes::ServiceSettingsParseError,
                format!("error converting payload to service settings - {}", e),
            ))
        }
    };
    info!(func = fn_name, package = PACKAGE_NAME, "payload parsed",);
    Ok(payload)
}
