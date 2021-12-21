use std::sync::Arc;

use tokio::fs::File;
use tokio_util::compat::*;

use arrow2::error::Result;
use arrow2::io::csv::read_async::*;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    use std::env;
    let args: Vec<String> = env::args().collect();

    let file_path = &args[1];

    let file = File::open(file_path).await?.compat();

    let mut reader = AsyncReaderBuilder::new().create_reader(file);

    let schema = Arc::new(infer_schema(&mut reader, None, true, &infer).await?);

    let mut rows = vec![ByteRecord::default(); 100];
    let rows_read = read_rows(&mut reader, 0, &mut rows).await?;

    let batch = deserialize_batch(
        &rows[..rows_read],
        schema.fields(),
        None,
        0,
        deserialize_column,
    )?;
    println!("{:?}", batch.column(0));
    Ok(())
}
