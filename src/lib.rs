use clap::{ArgMatches, Args, Command, FromArgMatches};
use glib::markup_escape_text;
use std::{collections::HashMap, env, error, fmt, io, io::prelude::*, process};
use urlencoding::decode;

pub type BoxResult<T> = Result<T, Box<dyn error::Error>>;

#[derive(Debug, Args)]
struct RofiArgs {
    /// Set display
    #[arg(short, long, default_value = ":0", env)]
    display: String,

    /// Set rofi prompt
    #[arg(short, long, env = "PINENTRY_USER_DATA")]
    prompt: Option<String>,
}

pub fn cmd() -> Command {
    let cli = clap::command!()
        .after_help(
            "INSTALL:\n  \
  1. Copy `pinentry-rofi` to `~/.local/bin` or `/urs/bin`.
  2. `chmod +x your/path/pinentry-rofi`.
  3. Set `pinentry-program` in `~/.gnupg/gpg-agent.conf`. For example:
     `pinentry-program <HOME>/.local/bin/pinentry-rofi`
  4. Restart gpg-agent `gpgconf --kill gpg-agent`",
        )
        .help_template(
            "\
{before-help}{name} {version}
{author-with-newline}
{usage-heading} {usage}
{after-help}

{all-args}
",
        );
    RofiArgs::augment_args(cli)
}

struct Writer<'a> {
    writer: Box<dyn Write + 'a>,
}

impl<'a> Writer<'a> {
    pub fn new() -> Self {
        let writer = Box::new(std::io::stdout());
        Writer { writer }
    }
}

impl Writer<'_> {
    fn assuan_send(&mut self, mesg: &str) -> io::Result<()> {
        writeln!(self.writer, "{}", mesg)?;
        self.writer.flush()?;
        Ok(())
    }
}

fn run_rofi(rofi_args: &mut HashMap<String, Option<String>>, writer: &mut Writer) -> io::Result<bool> {
    let args: Vec<&String> = rofi_args
        .iter()
        .flat_map(|(arg, val)| match val {
            Some(v) => vec![arg, v],
            None => vec![arg],
        })
        .collect();
    let output = process::Command::new("rofi").args(args).output()?;
    if output.status.success() {
        let pw = String::from_utf8(output.stdout)
            .expect("Error reading rofi stdout")
            .trim_end()
            .to_owned();
        if !pw.is_empty() {
            writer.assuan_send(format!("D {}", pw).as_ref())?
        }
    } else {
        let mut err = String::from_utf8(output.stderr)
            .expect("Error reading rofi stderr")
            .to_owned();
        if err.is_empty() {
            err.push_str("rofi")
        }
        writer.assuan_send(format!("ERR 83886179 Operation cancelled <{err}>").as_ref())?;
        return Ok(false);
    }
    Ok(true)
}

#[derive(Debug)]
struct UnknownAction {
    action: String,
    arg: String,
}

impl fmt::Display for UnknownAction {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Unknown assuan command. Action `{0}`. Argument `{1}`",
            self.action, self.arg
        )
    }
}

impl error::Error for UnknownAction {}

fn handle_command<'a>(
    action: &'a str,
    arg: &'a str,
    rofi_args: &'a mut HashMap<String, Option<String>>,
    writer: &'a mut Writer,
    is_test: &'a bool,
) -> BoxResult<()> {
    let mut ok = true;

    match (action, arg) {
        ("OPTION", arg) => {
            let (opt, val) = arg.split_once('=').unwrap_or_else(|| (&arg, ""));
            match opt {
                "ttyname" => env::set_var("GPG_TTY", val),
                "ttytype" => env::set_var("GPG_TERM", val),
                "lc-ctype" => env::set_var("LC_CTYPE", val),
                "lc-messages" => env::set_var("LC_MESSAGES", val),
                _ => {}
            };
        }
        ("GETINFO", "pid") => writer.assuan_send(format!("D {}", process::id()).as_ref())?,
        ("GETINFO", "ttyinfo") => writer.assuan_send(
            format!(
                "D {0} {1} {2}",
                env::var("GPG_TTY")?,
                env::var("GPG_TERM").unwrap_or("".into()),
                env::var("DISPLAY")?
            )
            .as_ref(),
        )?,
        ("GETINFO", "flavor") => writer.assuan_send("D keyring")?,
        ("GETINFO", "version") => writer.assuan_send(format!("D {}", env!("CARGO_PKG_VERSION")).as_ref())?,
        ("SETPROMPT", arg) => {
            if !rofi_args.contains_key("-p") {
                rofi_args.insert(String::from("-p"), Some(arg.replace(":", "")));
            }
        }
        ("SETDESC", arg) => {
            let unquoted = decode(arg).unwrap().into_owned().replace("\n", "\r");
            let decoded = markup_escape_text(&unquoted);
            rofi_args.insert(String::from("-mesg"), Some(decoded.as_str().to_owned()));
        }
        ("GETPIN", _) => {
            ok = if !is_test { run_rofi(rofi_args, writer)? } else { true };
        }
        ("SETERROR", arg) => {
            let sep = "\r***************************\r";
            rofi_args.entry(String::from("-mesg")).and_modify(|e| {
                let val = e.as_ref().unwrap();
                let prev_msg = val.rsplit_once(sep).unwrap_or_else(|| ("", val)).1;
                *e = Some([arg, prev_msg].join(sep))
            });
        }
        ("SETKEYINFO" | "BYE", _) => {}
        _ => ok = false,
    }

    if ok {
        writer.assuan_send("OK")?
    } else {
        writer.assuan_send("BYE")?;
        return Err(Box::new(UnknownAction {
            action: action.to_owned(),
            arg: arg.to_owned(),
        }));
    }
    Ok(())
}

pub fn pinentry(args_matches: &ArgMatches) -> BoxResult<()> {
    let args = RofiArgs::from_arg_matches(args_matches)
        .map_err(|err| err.exit())
        .unwrap();

    let mut rofi_args = HashMap::from([
        ("-dmenu".to_owned(), None),
        ("-display".to_owned(), Some(args.display.to_owned())),
        ("-input".to_owned(), Some("/dev/null".to_owned())),
        ("-password".to_owned(), None),
        ("-disable-history".to_owned(), None),
        ("-l".to_owned(), Some("0".to_owned())),
    ]);

    match args.prompt {
        Some(prompt) => {
            if !prompt.is_empty() {
                rofi_args.insert("-p".to_owned(), Some(prompt));
            }
        }
        None => {}
    }

    let mut writer = Writer::new();
    writer.assuan_send("OK Please go ahead")?;
    for line in io::stdin().lock().lines() {
        let cmd = line?;
        let (action, arg) = cmd.split_once(' ').unwrap_or_else(|| (&cmd, ""));
        let is_test = false;
        handle_command(action, arg, &mut rofi_args, &mut writer, &is_test)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, env, process};

    struct AssuanEntry {
        cmd: String,
        etalon_output: String,
        etalon_rofi_args: Option<HashMap<String, Option<String>>>,
    }

    fn prepare_test_handle_command(
        custom_prompt: Option<String>,
    ) -> (HashMap<String, Option<String>>, Vec<AssuanEntry>) {
        let mut assuan_mapping = vec![
            AssuanEntry {
                cmd: "OPTION grab".to_owned(),
                etalon_output: "OK".to_owned(),
                etalon_rofi_args: None,
            },
            AssuanEntry {
                cmd: "OPTION ttyname=/dev/pts/1".to_owned(),
                etalon_output: "OK".to_owned(),
                etalon_rofi_args: None,
            },
            AssuanEntry {
                cmd: "OPTION ttytype=tmux-256color".to_owned(),
                etalon_output: "OK".to_owned(),
                etalon_rofi_args: None,
            },
            AssuanEntry {
                cmd: "OPTION lc-messages=C".to_owned(),
                etalon_output: "OK".to_owned(),
                etalon_rofi_args: None,
            },
            AssuanEntry {
                cmd: "OPTION allow-external-password-cache".to_owned(),
                etalon_output: "OK".to_owned(),
                etalon_rofi_args: None,
            },
            AssuanEntry {
                cmd: "OPTION default-ok=_OK".to_owned(),
                etalon_output: "OK".to_owned(),
                etalon_rofi_args: None,
            },
            AssuanEntry {
                cmd: "OPTION default-cancel=_Cancel".to_owned(),
                etalon_output: "OK".to_owned(),
                etalon_rofi_args: None,
            },
            AssuanEntry {
                cmd: "OPTION default-yes=_Yes".to_owned(),
                etalon_output: "OK".to_owned(),
                etalon_rofi_args: None,
            },
            AssuanEntry {
                cmd: "OPTION default-no=_No".to_owned(),
                etalon_output: "OK".to_owned(),
                etalon_rofi_args: None,
            },
            AssuanEntry {
                cmd: "OPTION default-prompt=PIN:".to_owned(),
                etalon_output: "OK".to_owned(),
                etalon_rofi_args: None,
            },
            AssuanEntry {
                cmd: "OPTION default-pwmngr=_Save in password manager".to_owned(),
                etalon_output: "OK".to_owned(),
                etalon_rofi_args: None,
            },
            AssuanEntry {
                cmd: "OPTION default-cf-visi=Do you really want to make your \
                                        passphrase visible on the screen?"
                    .to_owned(),
                etalon_output: "OK".to_owned(),
                etalon_rofi_args: None,
            },
            AssuanEntry {
                cmd: "OPTION default-tt-visi=Make passphrase visible".to_owned(),
                etalon_output: "OK".to_owned(),
                etalon_rofi_args: None,
            },
            AssuanEntry {
                cmd: "OPTION default-tt-hide=Hide passphrase".to_owned(),
                etalon_output: "OK".to_owned(),
                etalon_rofi_args: None,
            },
            AssuanEntry {
                cmd: "OPTION touch-file=/run/user/1000/gnupg/S.gpg-agent".to_owned(),
                etalon_output: "OK".to_owned(),
                etalon_rofi_args: None,
            },
            AssuanEntry {
                cmd: "GETINFO pid".to_owned(),
                etalon_output: format!("D {}\nOK", process::id()),
                etalon_rofi_args: None,
            },
            AssuanEntry {
                cmd: "GETINFO ttyinfo".to_owned(),
                etalon_output: (format!("D /dev/pts/1 tmux-256color {0}\nOK", env!("DISPLAY"))),
                etalon_rofi_args: None,
            },
            AssuanEntry {
                cmd: "GETINFO flavor".to_owned(),
                etalon_output: "D keyring\nOK".to_owned(),
                etalon_rofi_args: None,
            },
            AssuanEntry {
                cmd: "GETINFO version".to_owned(),
                etalon_output: format!("D {}\nOK", env!("CARGO_PKG_VERSION")),
                etalon_rofi_args: None,
            },
            AssuanEntry {
                cmd: "SETPROMPT Passphrase:".to_owned(),
                etalon_output: "OK".to_owned(),
                etalon_rofi_args: Some(HashMap::from([
                    ("-dmenu".to_owned(), None),
                    ("-display".to_owned(), Some(":0".to_owned())),
                    ("-input".to_owned(), Some("/dev/null".to_owned())),
                    ("-password".to_owned(), None),
                    ("-disable-history".to_owned(), None),
                    ("-l".to_owned(), Some("0".to_owned())),
                    ("-p".to_owned(), Some("Passphrase".to_owned())),
                ])),
            },
            AssuanEntry {
                cmd: "SETDESC Please enter the passphrase for the ssh key%0A  \
                                ke:yf:in:ge:rp:ri:nt %22<email@yhoo.com>%22"
                    .to_owned(),
                etalon_output: "OK".to_owned(),
                etalon_rofi_args: Some(HashMap::from([
                    ("-dmenu".to_owned(), None),
                    ("-display".to_owned(), Some(":0".to_owned())),
                    ("-input".to_owned(), Some("/dev/null".to_owned())),
                    ("-password".to_owned(), None),
                    ("-disable-history".to_owned(), None),
                    ("-l".to_owned(), Some("0".to_owned())),
                    ("-p".to_owned(), Some("Passphrase".to_owned())),
                    (
                        "-mesg".to_owned(),
                        Some(
                            "Please enter the passphrase for the ssh key\r  \
                                                ke:yf:in:ge:rp:ri:nt &quot;&lt;email@yhoo.com&gt;&quot;"
                                .to_owned(),
                        ),
                    ),
                ])),
            },
            AssuanEntry {
                cmd: "GETPIN".to_owned(),
                etalon_output: "OK".to_owned(),
                etalon_rofi_args: Some(HashMap::from([
                    ("-dmenu".to_owned(), None),
                    ("-display".to_owned(), Some(":0".to_owned())),
                    ("-input".to_owned(), Some("/dev/null".to_owned())),
                    ("-password".to_owned(), None),
                    ("-disable-history".to_owned(), None),
                    ("-l".to_owned(), Some("0".to_owned())),
                    ("-p".to_owned(), Some("Passphrase".to_owned())),
                    (
                        "-mesg".to_owned(),
                        Some(
                            "Please enter the passphrase for the ssh key\r  \
                                                ke:yf:in:ge:rp:ri:nt &quot;&lt;email@yhoo.com&gt;&quot;"
                                .to_owned(),
                        ),
                    ),
                ])),
            },
            AssuanEntry {
                cmd: "SETERROR Bad Passphrase (try 2 of 3)".to_owned(),
                etalon_output: "OK".to_owned(),
                etalon_rofi_args: Some(HashMap::from([
                    ("-dmenu".to_owned(), None),
                    ("-display".to_owned(), Some(":0".to_owned())),
                    ("-input".to_owned(), Some("/dev/null".to_owned())),
                    ("-password".to_owned(), None),
                    ("-disable-history".to_owned(), None),
                    ("-l".to_owned(), Some("0".to_owned())),
                    ("-p".to_owned(), Some("Passphrase".to_owned())),
                    (
                        "-mesg".to_owned(),
                        Some(
                            "Bad Passphrase (try 2 of 3)\r***************************\r\
                             Please enter the passphrase for the ssh key\r  \
                             ke:yf:in:ge:rp:ri:nt &quot;&lt;email@yhoo.com&gt;&quot;"
                                .to_owned(),
                        ),
                    ),
                ])),
            },
            AssuanEntry {
                cmd: "SETKEYINFO".to_owned(),
                etalon_output: "OK".to_owned(),
                etalon_rofi_args: None,
            },
            AssuanEntry {
                cmd: "BYE".to_owned(),
                etalon_output: "OK".to_owned(),
                etalon_rofi_args: None,
            },
            AssuanEntry {
                cmd: "error".to_owned(),
                etalon_output: "BYE".to_owned(),
                etalon_rofi_args: None,
            },
        ];
        let mut rofi_args = HashMap::from([
            ("-dmenu".to_owned(), None),
            ("-display".to_owned(), Some(":0".to_owned())),
            ("-input".to_owned(), Some("/dev/null".to_owned())),
            ("-password".to_owned(), None),
            ("-disable-history".to_owned(), None),
            ("-l".to_owned(), Some("0".to_owned())),
        ]);

        if let Some(prompt) = custom_prompt {
            assuan_mapping[19].etalon_rofi_args = Some(HashMap::from([
                ("-dmenu".to_owned(), None),
                ("-display".to_owned(), Some(":0".to_owned())),
                ("-input".to_owned(), Some("/dev/null".to_owned())),
                ("-password".to_owned(), None),
                ("-disable-history".to_owned(), None),
                ("-l".to_owned(), Some("0".to_owned())),
                ("-p".to_owned(), Some(prompt.to_owned())),
            ]));
            assuan_mapping[20].etalon_rofi_args = Some(HashMap::from([
                ("-dmenu".to_owned(), None),
                ("-display".to_owned(), Some(":0".to_owned())),
                ("-input".to_owned(), Some("/dev/null".to_owned())),
                ("-password".to_owned(), None),
                ("-disable-history".to_owned(), None),
                ("-l".to_owned(), Some("0".to_owned())),
                ("-p".to_owned(), Some(prompt.to_owned())),
                (
                    "-mesg".to_owned(),
                    Some(
                        "Please enter the passphrase for the ssh key\r  \
                                                ke:yf:in:ge:rp:ri:nt &quot;&lt;email@yhoo.com&gt;&quot;"
                            .to_owned(),
                    ),
                ),
            ]));
            assuan_mapping[21].etalon_rofi_args = Some(HashMap::from([
                ("-dmenu".to_owned(), None),
                ("-display".to_owned(), Some(":0".to_owned())),
                ("-input".to_owned(), Some("/dev/null".to_owned())),
                ("-password".to_owned(), None),
                ("-disable-history".to_owned(), None),
                ("-l".to_owned(), Some("0".to_owned())),
                ("-p".to_owned(), Some(prompt.to_owned())),
                (
                    "-mesg".to_owned(),
                    Some(
                        "Please enter the passphrase for the ssh key\r  \
                                                ke:yf:in:ge:rp:ri:nt &quot;&lt;email@yhoo.com&gt;&quot;"
                            .to_owned(),
                    ),
                ),
            ]));
            assuan_mapping[22].etalon_rofi_args = Some(HashMap::from([
                ("-dmenu".to_owned(), None),
                ("-display".to_owned(), Some(":0".to_owned())),
                ("-input".to_owned(), Some("/dev/null".to_owned())),
                ("-password".to_owned(), None),
                ("-disable-history".to_owned(), None),
                ("-l".to_owned(), Some("0".to_owned())),
                ("-p".to_owned(), Some(prompt.to_owned())),
                (
                    "-mesg".to_owned(),
                    Some(
                        "Bad Passphrase (try 2 of 3)\r***************************\r\
                            Please enter the passphrase for the ssh key\r  \
                            ke:yf:in:ge:rp:ri:nt &quot;&lt;email@yhoo.com&gt;&quot;"
                            .to_owned(),
                    ),
                ),
            ]));
            rofi_args.insert("-p".to_owned(), Some(prompt.to_owned()));
        }
        (rofi_args, assuan_mapping)
    }

    fn process_test_handle_command(custom_prompt: Option<String>) -> Result<(), Box<(dyn std::error::Error)>> {
        let (mut rofi_args, assuan_mapping) = prepare_test_handle_command(custom_prompt);
        let is_test = true;
        let mut buf = Vec::new();
        let mut etalon_output = String::new();
        {
            let mut writer = super::Writer {
                writer: Box::new(&mut buf),
            };
            for entry in assuan_mapping.iter() {
                let (action, arg) = entry.cmd.split_once(' ').unwrap_or_else(|| (&entry.cmd, ""));
                if let Err(e) = super::handle_command(action, arg, &mut rofi_args, &mut writer, &is_test) {
                    assert!(format!("{}", e).contains("Unknown assuan command"))
                }

                etalon_output.push_str(&entry.etalon_output);
                etalon_output.push_str("\n");

                if let Some(etalon) = &entry.etalon_rofi_args {
                    assert_eq!(etalon.to_owned(), rofi_args, "Action: {action}. Arg: {arg}");
                }
            }
        }
        let output = std::str::from_utf8(buf.as_slice())?.to_owned();
        assert_eq!(etalon_output, output);
        Ok(())
    }

    #[test]
    fn test_handle_command() -> Result<(), Box<(dyn std::error::Error)>> {
        process_test_handle_command(None)?;
        Ok(())
    }

    #[test]
    fn test_handle_command_custom_prompt() -> Result<(), Box<(dyn std::error::Error)>> {
        process_test_handle_command(Some("custom-prompt".to_owned()))?;
        Ok(())
    }
}
