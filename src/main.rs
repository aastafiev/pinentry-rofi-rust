mod pinentry;

use pinentry::pinentry;

use std::{io, collections::HashMap};
use clap::Parser;


const VERSION: &str= "0.1.0";

/// A simple pinentry program using rofi.{n}
/// {n}INSTALL.{n}
/// 1. Copy `pinentry-rofi` to your `~/.local/bin`.{n}
/// 2. `chmod +x ~/.local/bin/pinentry-rofi`.{n}
/// 3. Set `pinentry-program` in `~/.gnupg/gpg-agent.conf`. For example:{n}
///    `pinentry-program <HOME>/.local/bin/pinentry-rofi`{n}
/// 4. Restart gpg-agent `gpgconf --kill gpg-agent`'''{n}
#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Set display
    #[clap(short, long, default_value = ":0", env)]
    display: String,

    /// Set rofi prompt
    #[clap(short, long, env = "PINENTRY_USER_DATA")]
    prompt: Option<String>,
}


fn main() -> io::Result<()> {
    let args = Args::parse();

    let mut rofi_args = HashMap::from([
        (String::from("-dmenu"), None),
        (String::from("-display"), Some(args.display.to_string())),
        (String::from("-input"), Some(String::from("/dev/null"))),
        (String::from("-password"), None),
        (String::from("-disable-history"), None),
        (String::from("-l"), Some(String::from("0"))),
    ]);

    match args.prompt {
        Some(prompt) => { rofi_args.insert(String::from("-prompt"), Some(prompt)); },
        None => {},
    }

    pinentry(&mut rofi_args)?;
    Ok(())
}
