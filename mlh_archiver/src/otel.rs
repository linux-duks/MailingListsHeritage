#![cfg(feature = "otel")]

use opentelemetry::{KeyValue, global, trace::TracerProvider as _};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{
    Resource,
    metrics::{MeterProviderBuilder, PeriodicReader, SdkMeterProvider},
    trace::{RandomIdGenerator, Sampler, SdkTracerProvider},
};
use opentelemetry_semantic_conventions::{SCHEMA_URL, attribute::SERVICE_VERSION};
use tracing_core::Level;
use tracing_opentelemetry::{MetricsLayer, OpenTelemetryLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

fn otlp_endpoint() -> String {
    std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:4318".to_string())
}

fn resource() -> Resource {
    Resource::builder()
        .with_service_name(env!("CARGO_PKG_NAME"))
        .with_schema_url(
            [
                KeyValue::new(SERVICE_VERSION, env!("CARGO_PKG_VERSION")),
                KeyValue::new("deployment.environment", "develop"),
            ],
            SCHEMA_URL,
        )
        .build()
}

fn init_meter_provider() -> SdkMeterProvider {
    let exporter = opentelemetry_otlp::MetricExporter::builder()
        .with_http()
        .with_endpoint(format!("{}/v1/metrics", otlp_endpoint()))
        .with_temporality(opentelemetry_sdk::metrics::Temporality::default())
        .build()
        .unwrap();

    let reader = PeriodicReader::builder(exporter)
        .with_interval(std::time::Duration::from_secs(30))
        .build();

    let stdout_reader =
        PeriodicReader::builder(opentelemetry_stdout::MetricExporter::default()).build();

    let meter_provider = MeterProviderBuilder::default()
        .with_resource(resource())
        .with_reader(reader)
        .with_reader(stdout_reader)
        .build();

    global::set_meter_provider(meter_provider.clone());

    meter_provider
}

fn init_tracer_provider() -> SdkTracerProvider {
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .with_endpoint(format!("{}/v1/traces", otlp_endpoint()))
        .build()
        .unwrap();

    // SimpleSpanProcessor exports synchronously on span end — no async runtime needed
    SdkTracerProvider::builder()
        .with_sampler(Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(
            1.0,
        ))))
        .with_id_generator(RandomIdGenerator::default())
        .with_resource(resource())
        .with_simple_exporter(exporter)
        .build()
}

pub fn init_tracing_subscriber() -> OtelGuard {
    let tracer_provider = init_tracer_provider();
    let meter_provider = init_meter_provider();

    let tracer = tracer_provider.tracer("tracing-otel-subscriber");

    tracing_subscriber::registry()
        .with(tracing_subscriber::filter::LevelFilter::from_level(
            Level::INFO,
        ))
        .with(tracing_subscriber::fmt::layer())
        .with(MetricsLayer::new(meter_provider.clone()))
        .with(OpenTelemetryLayer::new(tracer))
        .init();

    OtelGuard {
        tracer_provider,
        meter_provider,
    }
}

pub struct OtelGuard {
    tracer_provider: SdkTracerProvider,
    meter_provider: SdkMeterProvider,
}

impl Drop for OtelGuard {
    fn drop(&mut self) {
        if let Err(err) = self.tracer_provider.shutdown() {
            eprintln!("{err:?}");
        }
        if let Err(err) = self.meter_provider.shutdown() {
            eprintln!("{err:?}");
        }
    }
}
