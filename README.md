# Description

A simple pinentry program using rofi.

Inspired by [gist](https://gist.github.com/Cimbali/862a430a0f28ffe07f8ae618e8b73973) and [@plattfot/pinentry-rofi](https://github.com/plattfot/pinentry-rofi/tree/master)

## Dependencies

- Rust language for building (Packages: glib, urlencoding, clap)
- rofi

## Install

1. Copy `pinentry-rofi` to your `~/.local/bin`
2. `chmod +x ~/.local/bin/pinentry-rofi`
3. Set `pinentry-program` in `~/.gnupg/gpg-agent.conf`. For example:

    `pinentry-program <HOME>/.local/bin/pinentry-rofi`

4. Restart gpg-agent `gpgconf --kill gpg-agent`

## Documentation

Run `pinentry-rofi --help`
