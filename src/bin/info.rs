use geomedea::Reader;
use std::fs;
use std::io::BufReader;

type Error = Box<dyn std::error::Error>;
type Result<T> = std::result::Result<T, Error>;

fn main() -> Result<()> {
    let mut args = std::env::args();
    let bin_name = args.next().expect("Missing bin name");
    let input_path = args.next().unwrap_or_else(|| {
        panic!("Missing input path.\nUsage:\n\t`{bin_name} <input-path.geomedea>`")
    });

    let file_size = fs::metadata(&input_path)?.len();
    let input = BufReader::new(std::fs::File::open(&input_path)?);
    let reader = Reader::new(input)?;
    let info = reader.info();
    eprintln!("info: {info:?}");

    let header_size = info.header_size()?;
    eprintln!("        file_size: {:?}", file_size);
    eprintln!("      header_size: {header_size}");
    eprintln!("       index_size: {}", info.index_size());
    eprintln!(
        "feature_data_size: {}",
        file_size - header_size - info.index_size()
    );

    eprintln!("done");
    Ok(())
}
