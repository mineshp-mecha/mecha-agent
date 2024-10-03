use anyhow::{bail, Result};
use opentelemetry::KeyValue;
use opentelemetry_otlp::{ExportConfig, WithExportConfig};
use opentelemetry_sdk::{metrics, runtime, Resource};
use std::time::Duration;
pub fn init_otlp_configuration(
    otlp_collector_addr: String,
    export_metrics_duration: u64,
) -> Result<metrics::SdkMeterProvider, opentelemetry::metrics::MetricsError> {
    let export_config = ExportConfig {
        endpoint: otlp_collector_addr,
        ..ExportConfig::default()
    };

    let duration = Duration::from_secs(export_metrics_duration); // define duration to export metrics after this duration

    // let exporter = opentelemetry_otlp::new_exporter().
    opentelemetry_otlp::new_pipeline()
        .metrics(runtime::Tokio)
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_export_config(export_config),
        )
        .with_period(duration)
        .with_resource(Resource::new(vec![KeyValue::new(
            opentelemetry_semantic_conventions::resource::SERVICE_NAME,
            "basic-otlp-metrics-example",
        )]))
        .build()
}

pub fn init_logs_config(
    otlp_collector_address: String,
) -> Result<opentelemetry_sdk::logs::LoggerProvider> {
    match opentelemetry_otlp::new_pipeline()
        .logging()
        .with_resource(Resource::new(vec![
            KeyValue::new(
                opentelemetry_semantic_conventions::resource::SERVICE_NAME,
                "mecha-agent-service",
            ),
            KeyValue::new("stream_name", "log_stream"),
        ]))
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint(otlp_collector_address),
        )
        .install_batch(opentelemetry_sdk::runtime::Tokio)
    {
        Ok(provider) => return Ok(provider),
        Err(e) => {
            eprintln!("error initializing logs config: {:?}", e);
            bail!(e);
        }
    };
}
