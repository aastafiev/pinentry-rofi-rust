use std::io;

use pinentry_rofi::{cmd, pinentry};

fn main() -> io::Result<()> {
    let matches = cmd().get_matches();
    pinentry(&matches)?;
    Ok(())
}
