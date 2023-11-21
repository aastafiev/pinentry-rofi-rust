mod pinentry;

use pinentry::pinentry;

use clap::{Command, Args, FromArgMatches};
// use clap::Parser;
use std::{io, collections::HashMap};
// use std::io;


const VERSION: &str= env!("CARGO_PKG_VERSION");


#[derive(Debug, Args)]
struct RofiArgs {
    /// Set display
    #[arg(short, long, default_value = ":0", env)]
    display: String,

    /// Set rofi prompt
    #[arg(short, long, env = "PINENTRY_USER_DATA")]
    prompt: Option<String>,
}


fn cmd() -> Command {
    let cli = clap::command!()
        .after_help("INSTALL:\n  \
  1. Copy `pinentry-rofi` to your `~/.local/bin`.
  2. `chmod +x ~/.local/bin/pinentry-rofi`.
  3. Set `pinentry-program` in `~/.gnupg/gpg-agent.conf`. For example:
     `pinentry-program <HOME>/.local/bin/pinentry-rofi`
  4. Restart gpg-agent `gpgconf --kill gpg-agent`")
        .help_template("\
{before-help}{name} {version}
{author-with-newline}
{usage-heading} {usage}
{after-help}

{all-args}
");
    RofiArgs::augment_args(cli)
}


fn main() -> io::Result<()> {
    let matches = cmd().get_matches();
    let args = RofiArgs::from_arg_matches(&matches)
        .map_err(|err| err.exit())
        .unwrap();

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
