use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
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

fn benchmark(c: &mut Criterion) {
    c.bench_function("write (compressed)", |b| {
        b.iter_batched(
            || {
                let input = BufReader::new(File::open("test_fixtures/places.json").unwrap());
                GeoJsonReader(input)
            },
            |geojson| write(black_box(geojson), true),
            BatchSize::LargeInput,
        );
    });
    c.bench_function("write (uncompressed)", |b| {
        b.iter_batched(
            || {
                let input = BufReader::new(File::open("test_fixtures/places.json").unwrap());
                GeoJsonReader(input)
            },
            |geojson| write(black_box(geojson), false),
            BatchSize::LargeInput,
        );
    });
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
