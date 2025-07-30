# pochta

Command line interface for Gmail imap

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

## Setup

1. Create new project using [google developer console](https://console.developers.google.com/)
2. Download oauth client info. There will be client id and client secret.
3. In `Credentials` section select `WebClient` and add this redirect URI: `https://google.github.io/gmail-oauth2-tools/html/oauth2.dance.html`
4. Using this [guide](https://github.com/google/gmail-oauth2-tools/wiki/OAuth2DotPyRunThrough) get refresh token

## Build

```console
CLIENT_ID="<value>" CLIENT_SECRET="<value>" REFRESH_TOKEN="<value>" EMAIL="your_email@example.com" cargo build
```

## Usage

In `pochta` you enter raw imap commands.
You can learn about imap commands [here](https://www.rfc-editor.org/rfc/rfc3501)
(not all commands are there)
