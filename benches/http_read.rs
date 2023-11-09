use criterion::{black_box, criterion_group, criterion_main, Criterion};
use futures_util::StreamExt;
pub use geomedea::{Bounds, HttpReader, LngLat};
use geozero::geomedea::GeomedeaWriter as GeozeroWriter;
use geozero::{geojson::GeoJsonReader, GeozeroDatasource};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::time::Duration;
use tempfile::TempDir;
use yocalhost::ThrottledServer;

struct TestServer {
    web_root: TempDir,
    server: ThrottledServer,
}

impl TestServer {
    fn new(port: u16) -> Self {
        let web_root = tempfile::tempdir().unwrap();
        let server = ThrottledServer::new(
            port,
            Duration::from_millis(100),
            50_000_000 / 8,
            "./test_fixtures",
        );
        Self { web_root, server }
    }
}

fn write(mut geojson: GeoJsonReader<BufReader<File>>, is_compressed: bool, output_path: &Path) {
    let mut output = std::io::BufWriter::new(File::create(output_path).unwrap());
    let mut writer = GeozeroWriter::new(&mut output, is_compressed).unwrap();
    // Artificially small page size to make sure we're exercising paging code paths
    writer.set_page_size_goal(8 * 1024);
    geojson.process(&mut writer).unwrap();
    writer.finish().unwrap();
}

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
    let test_server = TestServer::new(6868);
    let test_port = test_server.server.port();

    let compressed_url = format!("http://localhost:{test_port}/USCounties-compressed.geomedea");
    let uncompressed_url = format!("http://localhost:{test_port}/USCounties-uncompressed.geomedea");

    // {
    //     let input = BufReader::new(File::open("test_fixtures/places.json").unwrap());
    //     let geojson = GeoJsonReader(input);
    //     let output_path = test_server.web_root.path().join("compressed.geomedea");
    //     write(geojson, true, &output_path)
    // }
    // {
    //     let input = BufReader::new(File::open("test_fixtures/places.json").unwrap());
    //     let geojson = GeoJsonReader(input);
    //     let output_path = test_server.web_root.path().join("uncompressed.geomedea");
    //     write(geojson, false, &output_path)
    // }

    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.spawn(async move {
        test_server.server.serve().await;
    });

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
