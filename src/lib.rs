use std::{io, io::prelude::*, process, collections::HashMap, env};
use urlencoding::decode;
use glib::markup_escape_text;
use clap::{Command, Args, FromArgMatches, ArgMatches};

const VERSION: &str= env!("CARGO_PKG_VERSION");

#[derive(Debug, Args)]
struct RofiArgs {
    /// Set display
    #[arg(short, long, default_value = ":0", env)]
    display: String,

    /// Set rofi prompt
    #[arg(short, long, default_value = "", env = "PINENTRY_USER_DATA")]
    prompt: Option<String>,
}


pub fn cmd() -> Command {
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


fn assuan_send(mesg: String) -> io::Result<()> {
    println!("{}", mesg);
    io::stdout().flush()?;
    Ok(())
}


fn run_rofi(rofi_args: &mut HashMap<String, Option<String>>) -> io::Result<bool> {
    let args: Vec<&String> = rofi_args.iter().flat_map(|(arg, val)| {
        match val {
            Some(v) => vec![arg, v],
            None => vec![arg],
        }
    }).collect();
    let output = process::Command::new("rofi").args(args).output()?;
    if output.status.success() {
        let pw = String::from_utf8(output.stdout)
            .expect("Error reading rofi stdout")
            .trim_end()
            .to_string();
        if !pw.is_empty() { assuan_send(format!("D {}", pw))? }
    } else {
        let mut err = String::from_utf8(output.stderr)
            .expect("Error reading rofi stderr")
            .to_string();
        if err.is_empty() { err.push_str("rofi") }
        assuan_send(format!("ERR 83886179 Operation cancelled <{err}>"))?;
        return Ok(false)
    }
    Ok(true)
}


pub fn handle_command(action: &str, arg: &str, rofi_args: &mut HashMap<String, Option<String>>) -> io::Result<()> {
    let mut ok = true;

    match (action, arg) {
        ("OPTION", arg) => {
            let (opt, val) = arg.split_once('=').unwrap_or_else(|| (&arg, ""));
            match opt {
                "ttyname" => { env::set_var("GPG_TTY", val) },
                "ttytype" => { env::set_var("GPG_TERM", val) },
                "lc-ctype" => { env::set_var("LC_CTYPE", val) },
                "lc-messages" => { env::set_var("LC_MESSAGES", val) },
                _ => {},
            };
        },
        ("GETINFO", "pid") => { assuan_send(format!("D {}", process::id()))? },
        ("GETINFO", "ttyinfo") => {
            assuan_send(format!("D {0} {1} {2}",
                                    env::var("GPG_TTY").expect("GPG_TTY environment variable not set"),
                                    env::var("GPG_TERM").unwrap_or("".to_string()),
                                    env::var("DISPLAY").expect("DISPLAY environment variable not set")))?
        },
        ("GETINFO", "flavor") => { assuan_send("D keyring".to_string())? },
        ("GETINFO", "version") => { assuan_send(format!("D {}", VERSION))? },
        ("SETPROMPT", arg) => {
            if !rofi_args.contains_key("-p") {
                rofi_args.insert(String::from("-p"), Some(arg.replace(":", "")));
            }
        },
        ("SETDESC", arg) => {
            let unquoted = decode(arg).unwrap().into_owned().replace("\n", "\r");
            let decoded = markup_escape_text(&unquoted);
            rofi_args.insert(String::from("-mesg"), Some(decoded.as_str().to_string()));
        },
        ("GETPIN", _) => { ok = run_rofi(rofi_args).unwrap(); },
        ("SETERROR", arg) => {
            let sep = "\r***************************\r";
            rofi_args
                .entry(String::from("-mesg"))
                .and_modify(|e| {
                    let val = e.as_ref().unwrap();
                    let prev_msg = val.rsplit_once(sep).unwrap_or_else(|| ("", val)).1;
                    *e = Some([arg, prev_msg].join(sep))
            });
        },
        ("SETKEYINFO" | "BYE", _) => {},
        _ => { ok = false },
    }

    if ok { assuan_send("OK".to_string())? }
    else {
        assuan_send("BYE".to_string())?;
        process::exit(1);
    }
    Ok(())
}


pub fn pinentry(args_matches: &ArgMatches) -> io::Result<()> {
    let args = RofiArgs::from_arg_matches(args_matches)
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

    assuan_send("OK Please go ahead".to_string())?;

    for line in io::stdin().lock().lines() {
        let row = line?;
        let (action, arg) = row.split_once(' ').unwrap_or_else(|| (&row, ""));
        handle_command(action, arg, &mut rofi_args)?;
    }
    Ok(())
}
