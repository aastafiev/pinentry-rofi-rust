use pinentry_rofi::{cmd, pinentry, BoxResult};

fn main() -> BoxResult<()> {
    let matches = cmd().get_matches();
    pinentry(&matches)?;
    Ok(())
}
