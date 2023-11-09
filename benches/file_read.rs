use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
pub use geomedea::{Bounds, LngLat, Reader};
use geozero::geomedea::GeomedeaWriter as GeozeroWriter;
use geozero::{geojson::GeoJsonReader, GeozeroDatasource};
use std::fs::File;
use std::io::BufReader;

fn write(mut geojson: GeoJsonReader<BufReader<File>>, is_compressed: bool) -> Vec<u8> {
    let mut output = vec![];
    let mut writer = GeozeroWriter::new(&mut output, is_compressed).unwrap();
    // Artificially small page size to make sure we're exercising paging code paths
    writer.set_page_size_goal(8 * 1024);
    geojson.process(&mut writer).unwrap();
    writer.finish().unwrap();
    output
}

fn select_all(bytes: &[u8]) {
    let reader = Reader::new(bytes).unwrap();
    let mut features = reader.select_all().unwrap();
    let mut count = 0;
    while let Some(feature) = features.next().unwrap() {
        black_box(feature);
        count += 1;
    }
    assert_eq!(count, 1249);
}

fn select_bbox(bytes: &[u8]) {
    let reader = Reader::new(bytes).unwrap();
    let mut features = reader
        .select_bbox(&Bounds::from_corners(
            &LngLat::degrees(90.0, 40.0),
            &LngLat::degrees(100.0, 50.0),
        ))
        .unwrap();
    let mut count = 0;
    while let Some(_feature) = features.next().unwrap() {
        black_box(_feature);
        count += 1;
    }
    assert_eq!(count, 3);
}

fn benchmark(c: &mut Criterion) {
    c.bench_function("select_all (compressed file)", |b| {
        b.iter_batched(
            || {
                let input = BufReader::new(File::open("test_fixtures/places.json").unwrap());
                let geojson = GeoJsonReader(input);
                write(geojson, true)
            },
            |bytes| select_all(&bytes),
            BatchSize::LargeInput,
        );
    });
    c.bench_function("select_all (uncompressed file)", |b| {
        b.iter_batched(
            || {
                let input = BufReader::new(File::open("test_fixtures/places.json").unwrap());
                let geojson = GeoJsonReader(input);
                write(geojson, false)
            },
            |bytes| select_all(&bytes),
            BatchSize::LargeInput,
        );
    });
    c.bench_function("select_bbox (compressed file)", |b| {
        b.iter_batched(
            || {
                let input = BufReader::new(File::open("test_fixtures/places.json").unwrap());
                let geojson = GeoJsonReader(input);
                write(geojson, true)
            },
            |bytes| select_bbox(&bytes),
            BatchSize::LargeInput,
        );
    });
    c.bench_function("select_bbox (uncompressed file)", |b| {
        b.iter_batched(
            || {
                let input = BufReader::new(File::open("test_fixtures/places.json").unwrap());
                let geojson = GeoJsonReader(input);
                write(geojson, false)
            },
            |bytes| select_bbox(&bytes),
            BatchSize::LargeInput,
        );
    });
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
