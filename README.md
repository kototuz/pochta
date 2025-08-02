# pochta

Command line interface for Gmail imap/smtp

```console
$ pochta
...
>> select inbox
* FLAGS (\Answered \Flagged \Draft \Deleted \Seen $NotPhishing $Phishing)
* OK [PERMANENTFLAGS (\Answered \Flagged \Draft \Deleted \Seen $NotPhishing $Phishing \*)] Flags permitted.
* OK [UIDVALIDITY 1] UIDs valid.
* 1 EXISTS
* 0 RECENT
* OK [UIDNEXT 1234] Predicted next UID.
* OK [HIGHESTMODSEQ 100234]
K0001 OK [READ-WRITE] inbox selected. (Success)
>> ...
```

## Build

Run "build script" and follow instructions.
It will build an actual executable
```console
$ cargo run --bin make
```

## Usage

``` console
$ ./target/release/pochta -help
$ ./target/release/pochta
```

In `pochta` you enter raw imap/smtp commands.
You can learn about imap [here](https://www.rfc-editor.org/rfc/rfc3501)
(not all commands are there).
About smtp [here](https://www.rfc-editor.org/rfc/rfc5321.html)
