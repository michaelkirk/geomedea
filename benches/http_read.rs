use criterion::{black_box, criterion_group, criterion_main, Criterion};
use futures_util::StreamExt;
pub use geomedea::{Bounds, HttpReader, LngLat};
use std::time::Duration;
use yocalhost::ThrottledServer;

async fn select_all(url: &str) {
    let mut reader = HttpReader::open(url).await.unwrap();
    let mut features = reader.select_all().await.unwrap();
    let mut count = 0;
    while let Some(feature) = features.next().await.transpose().unwrap() {
        black_box(feature);
        count += 1;
    }
    assert_eq!(count, 3221);
}

async fn select_bbox(url: &str) {
    let mut reader = HttpReader::open(url).await.unwrap();
    let mut features = reader
        .select_bbox(&Bounds::from_corners(
            &LngLat::degrees(-86.0, 10.0),
            &LngLat::degrees( -85.0, 40.0),
        ))
        .await
        .unwrap();
    let mut count = 0;
    while let Some(feature) = features.next().await.transpose().unwrap() {
        black_box(feature);
        count += 1;
    }
    assert_eq!(count, 140);
}

fn benchmark(c: &mut Criterion) {
    env_logger::builder().format_timestamp_millis().init();

    let test_server_port = 6868;
    let server = ThrottledServer::new(
        test_server_port,
        Duration::from_millis(10),
        500_000_000 / 8,
        "./test_fixtures",
    );
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.spawn(async move {
        server.serve().await;
    });

    let compressed_url = format!("http://localhost:{test_server_port}/USCounties-compressed.geomedea");
    let uncompressed_url = format!("http://localhost:{test_server_port}/USCounties-uncompressed.geomedea");

    c.bench_function("HTTP select_all (compressed)", |b| {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        b.to_async(runtime).iter(|| select_all(&compressed_url))
    });

    c.bench_function("HTTP select_all (uncompressed)", |b| {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        b.to_async(runtime).iter(|| select_all(&uncompressed_url))
    });

    c.bench_function("HTTP select_bbox (compressed)", |b| {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        b.to_async(runtime).iter(|| select_bbox(&compressed_url))
    });

    c.bench_function("HTTP select_bbox (uncompressed)", |b| {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        b.to_async(runtime).iter(|| select_bbox(&uncompressed_url))
    });
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
