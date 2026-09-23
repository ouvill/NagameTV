use std::io::{Read, Write};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    const PACKET_BYTES: usize = 188;
    let mut filter = tsreadex::Filter::new(1)?;
    let mut input = std::io::stdin().lock();
    let mut output = std::io::stdout().lock();
    let mut packet = [0; PACKET_BYTES];
    loop {
        match input.read_exact(&mut packet) {
            Ok(()) => output.write_all(filter.push(&packet)?)?,
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}
